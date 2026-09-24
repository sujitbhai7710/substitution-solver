//! Word-pattern dictionary attack (Olson 2007 / quipqiup style) as a
//! *primary* solver.
//!
//! Backtracking search over pattern-consistent dictionary words:
//!   - candidate words per cipher word from the pattern index, filtered by
//!     clue locks up front (word-disabling fallback: cipher words with no
//!     dictionary candidates, e.g. proper nouns, are left for the
//!     statistical phase instead of failing the search);
//!   - dynamic most-constrained-variable ordering ("lazy planner"): at each
//!     node the unassigned word with the fewest currently-compatible
//!     candidates is expanded next, with dead-end detection;
//!   - forward checking against the partial cipher<->plain bijection;
//!   - returns the top-K complete assignments by word unigram score; the
//!     caller re-ranks them with the trigram (with spaces) posterior.
//!
//! Timeboxed by a node budget; deterministic given the wordlist.

use crate::codec::{Puzzle, Slot};
use crate::words::{pattern_of, score_word, word_data, WordData};

/// Simple SplitMix64 RNG (local copy to avoid cross-module plumbing).
pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: u32) -> u32 {
        (self.next() % n as u64) as u32
    }
}

/// One dictionary-attack solution: partial key (255 = unmapped cipher
/// letter), word-score, and coverage count.
pub struct DictHit {
    pub key: [u8; 26],
    pub score: f64,
    pub mapped_letters: u32,
}

struct CandWord {
    slots: Vec<Slot>,
    cands: Vec<u32>,
}

fn word_tok_string(slots: &[Slot]) -> String {
    slots
        .iter()
        .map(|s| match s {
            Slot::Letter(c) => (b'A' + c) as char,
            Slot::Apos => '\'',
        })
        .collect()
}

/// Build the unique cipher-word list with pattern candidates.
/// Words with no candidates (proper nouns / OOV) are dropped here — the
/// statistical phase handles their letters.
fn build_candidates(
    puz: &Puzzle,
    wd: &WordData,
    locked_c: &[bool; 26],
    lock_plain: &[u8; 26],
    cand_cap: usize,
) -> Vec<CandWord> {
    let mut seen: Vec<Vec<Slot>> = Vec::new();
    let mut out: Vec<CandWord> = Vec::new();
    for w in &puz.words {
        let mut tok: Vec<u8> = Vec::with_capacity(w.slots.len());
        let mut has_letter = false;
        for s in &w.slots {
            match s {
                Slot::Letter(c) => {
                    tok.push(b'A' + c);
                    has_letter = true;
                }
                Slot::Apos => tok.push(b'\''),
            }
        }
        if !has_letter {
            continue;
        }
        if seen.iter().any(|s| slots_eq(s, &w.slots)) {
            continue;
        }
        seen.push(w.slots.clone());
        let pat = pattern_of(&tok);
        let mut cands: Vec<u32> = Vec::new();
        if let Some(list) = wd.patterns.get(&pat) {
            'outer: for &wi in list.iter().take(cand_cap) {
                let cw = &wd.words[wi as usize];
                let mut ci = 0usize;
                for s in &w.slots {
                    match s {
                        Slot::Apos => {
                            if cw[ci] != b'\'' {
                                continue 'outer;
                            }
                            ci += 1;
                        }
                        Slot::Letter(c) => {
                            if locked_c[*c as usize] && cw[ci] != b'A' + lock_plain[*c as usize]
                            {
                                continue 'outer;
                            }
                            ci += 1;
                        }
                    }
                }
                cands.push(wi);
            }
        }
        if cands.is_empty() {
            continue; // word-disabling fallback
        }
        out.push(CandWord {
            slots: w.slots.clone(),
            cands,
        });
    }
    out
}

fn slots_eq(a: &[Slot], b: &[Slot]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b.iter()).all(|(x, y)| match (x, y) {
            (Slot::Letter(p), Slot::Letter(q)) => p == q,
            (Slot::Apos, Slot::Apos) => true,
            _ => false,
        })
}

struct Dfs<'a> {
    wd: &'a WordData,
    words: Vec<CandWord>,
    c2p: [i8; 26],
    p2c: [i8; 26],
    done: Vec<bool>,
    top: Vec<DictHit>,
    top_k: usize,
    worst_top: f64,
    nodes: u64,
    max_nodes: u64,
    expand_cap: usize,
    trace: bool,
}

impl<'a> Dfs<'a> {
    /// Check candidate `ci` of word `wi` against the current partial map.
    fn compatible(&self, wi: usize, ci: u32) -> bool {
        let w = &self.words[wi];
        let cw = &self.wd.words[ci as usize];
        // Candidate must have exactly one byte per slot (guaranteed by the
        // shared pattern, but check defensively).
        if cw.len() != w.slots.len() {
            return false;
        }
        for (s, &ch) in w.slots.iter().zip(cw.iter()) {
            match s {
                Slot::Apos => {
                    if ch != b'\'' {
                        return false;
                    }
                }
                Slot::Letter(c) => {
                    if ch == b'\'' {
                        return false;
                    }
                    let p = (ch - b'A') as usize;
                    let c = *c as usize;
                    if self.c2p[c] >= 0 && self.c2p[c] != p as i8 {
                        return false;
                    }
                    if self.p2c[p] >= 0 && self.p2c[p] != c as i8 {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Count compatible candidates of word `wi`, stopping early at `limit`.
    fn count_compat(&self, wi: usize, limit: usize) -> usize {
        let mut c = 0usize;
        for &ci in &self.words[wi].cands {
            if self.compatible(wi, ci) {
                c += 1;
                if c >= limit {
                    break;
                }
            }
        }
        c
    }

    /// Dynamic MRV: returns the unassigned word with fewest compatible
    /// candidates, or None if all assigned (done) or a dead end was found.
    /// Returns (word_index, is_dead_end).
    fn pick(&self) -> Option<(usize, bool)> {
        let mut best: Option<usize> = None;
        let mut best_c = usize::MAX;
        let mut any = false;
        for wi in 0..self.words.len() {
            if self.done[wi] {
                continue;
            }
            any = true;
            let c = self.count_compat(wi, best_c);
            if c == 0 {
                return Some((wi, true));
            }
            if c < best_c {
                best_c = c;
                best = Some(wi);
                if c == 1 {
                    break;
                }
            }
        }
        if !any {
            return None; // all done
        }
        best.map(|wi| (wi, false))
    }

    /// Assign candidate `ci` of word `wi`; returns undo information.
    fn assign(&mut self, wi: usize, ci: u32) -> Vec<(u8, u8)> {
        let w = &self.words[wi];
        let cw = &self.wd.words[ci as usize];
        let mut set = Vec::new();
        for (s, &ch) in w.slots.iter().zip(cw.iter()) {
            if let Slot::Letter(c) = s {
                let p = ch - b'A';
                let c = *c as usize;
                if self.c2p[c] < 0 {
                    self.c2p[c] = p as i8;
                    self.p2c[p as usize] = c as i8;
                    set.push((c as u8, p));
                }
            }
        }
        set
    }

    fn unassign(&mut self, set: &[(u8, u8)]) {
        for &(c, p) in set {
            self.c2p[c as usize] = -1;
            self.p2c[p as usize] = -1;
        }
    }

    fn record(&mut self, score: f64) {
        if self.top.len() >= self.top_k && score <= self.worst_top {
            return;
        }
        if self.top.is_empty() && std::env::var("SOLVE_PROFILE").is_ok() {
            eprintln!("[profile] dict first hit at node {}", self.nodes);
        }
        let mut key = [255u8; 26];
        let mut mapped = 0u32;
        for c in 0..26 {
            if self.c2p[c] >= 0 {
                key[c] = self.c2p[c] as u8;
                mapped += 1;
            }
        }
        self.top.push(DictHit {
            key,
            score,
            mapped_letters: mapped,
        });
        self.top.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        self.top.truncate(self.top_k);
        self.worst_top = self
            .top
            .last()
            .map(|h| h.score)
            .unwrap_or(f64::NEG_INFINITY);
    }

    fn dfs(&mut self, depth: usize, score: f64) {
        // Node budget with early abort: the first complete assignment
        // almost always appears within a few thousand nodes (measured
        // p90 = 48 nodes, max = 2,997 on 103 corpus puzzles). Past 10k
        // nodes with zero hits the puzzle isn't dictionary-attack
        // amenable (too much OOV); stop burning budget and let the
        // statistical phase handle it. This caps the worst case (~19s)
        // without touching the common case.
        let limit = if self.top.is_empty() {
            self.max_nodes.min(10_000)
        } else {
            self.max_nodes
        };
        if self.nodes >= limit {
            return;
        }
        self.nodes += 1;
        let (wi, dead) = match self.pick() {
            None => {
                self.record(score);
                return;
            }
            Some((wi, true)) => {
                if self.trace {
                    eprintln!(
                        "[trace] dead at depth {} on word {}",
                        depth,
                        word_tok_string(&self.words[wi].slots)
                    );
                }
                return;
            }
            Some((wi, false)) => (wi, false),
        };
        let _ = dead;
        if self.trace && depth < 4 {
            eprintln!(
                "[trace] depth {} expand word {} ({} cands)",
                depth,
                word_tok_string(&self.words[wi].slots),
                self.words[wi].cands.len()
            );
        }
        self.done[wi] = true;
        // Collect compatible candidates (bounded by expand_cap).
        let mut compat: Vec<u32> = Vec::new();
        for &ci in &self.words[wi].cands {
            if self.compatible(wi, ci) {
                compat.push(ci);
                if compat.len() >= self.expand_cap {
                    break;
                }
            }
        }
        if self.trace && depth < 4 {
            eprintln!("[trace] depth {} word {} compat={}", depth, word_tok_string(&self.words[wi].slots), compat.len());
        }
        for ci in compat {
            let set = self.assign(wi, ci);
            let ws = score_word(self.wd, &self.wd.words[ci as usize]) as f64;
            self.dfs(depth + 1, score + ws);
            self.unassign(&set);
            if self.nodes >= self.max_nodes {
                break;
            }
        }
        self.done[wi] = false;
    }
}

/// Run the dictionary attack; return up to `top_k` assignments by word
/// score (best first). Deterministic.
pub fn dict_attack(
    puz: &Puzzle,
    locked_c: &[bool; 26],
    lock_plain: &[u8; 26],
    max_nodes: u64,
    top_k: usize,
    cand_cap: usize,
) -> Vec<DictHit> {
    dict_attack_inner(puz, locked_c, lock_plain, max_nodes, top_k, cand_cap, false)
}

fn dict_attack_inner(
    puz: &Puzzle,
    locked_c: &[bool; 26],
    lock_plain: &[u8; 26],
    max_nodes: u64,
    top_k: usize,
    cand_cap: usize,
    trace: bool,
) -> Vec<DictHit> {
    let wd = word_data();
    let words = build_candidates(puz, wd, locked_c, lock_plain, cand_cap);
    if words.is_empty() {
        return Vec::new();
    }
    let n = words.len();
    let mut dfs = Dfs {
        wd,
        words,
        c2p: [-1; 26],
        p2c: [-1; 26],
        done: vec![false; n],
        top: Vec::new(),
        top_k: top_k.max(1),
        worst_top: f64::NEG_INFINITY,
        nodes: 0,
        max_nodes,
        expand_cap: 120,
        trace,
    };
    for c in 0..26 {
        if locked_c[c] {
            let p = lock_plain[c] as usize;
            dfs.c2p[c] = p as i8;
            dfs.p2c[p] = c as i8;
        }
    }
    dfs.dfs(0, 0.0);
    if trace {
        eprintln!(
            "[trace] done: nodes={} hits={}",
            dfs.nodes,
            dfs.top.len()
        );
    }
    dfs.top
}

/// Backwards-compatible seed: best dict-attack key with unmapped letters
/// filled randomly (bijectively), or None if the attack found nothing.
pub fn pattern_seed(
    puz: &Puzzle,
    locked_c: &[bool; 26],
    lock_plain: &[u8; 26],
    max_nodes: u64,
    rng: &mut Rng,
) -> Option<[u8; 26]> {
    let hits = dict_attack(puz, locked_c, lock_plain, max_nodes, 1, 2000);
    let h = hits.into_iter().next()?;
    let mut used = [false; 26];
    let mut key = [0u8; 26];
    for c in 0..26 {
        if h.key[c] != 255 {
            key[c] = h.key[c];
            used[key[c] as usize] = true;
        }
    }
    let mut free: Vec<u8> = (0..26u8).filter(|p| !used[*p as usize]).collect();
    for i in (1..free.len()).rev() {
        let j = rng.below((i + 1) as u32) as usize;
        free.swap(i, j);
    }
    let mut fi = 0;
    for c in 0..26 {
        if h.key[c] == 255 {
            key[c] = free[fi];
            fi += 1;
        }
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::parse;

    #[test]
    fn dict_attack_finds_cryptoquip() {
        let puz = parse("XPJ XLCSUH XLYX VJEOH RJ CV XLWB'FW HWPCSU XLCSUH PLCEW XLWB'FW PYEOCSU: SWWREW YSR XFWYR.");
        let hits = dict_attack(&puz, &[false; 26], &[0; 26], 200_000, 3, 2000);
        assert!(!hits.is_empty(), "dict attack found no solution");
        // X->T, L->H, W->E, B->Y, F->R (THEY'RE)
        let k = &hits[0].key;
        assert_eq!(k[(b'X' - b'A') as usize], b'T' - b'A');
        assert_eq!(k[(b'L' - b'A') as usize], b'H' - b'A');
    }
}
