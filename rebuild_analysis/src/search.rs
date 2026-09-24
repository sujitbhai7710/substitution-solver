//! Round-6 search: small-table, all-WASM solver.
//!
//! Scorer: character quadgram log-probs over 27 symbols (A-Z + space,
//! 27^4 quantized table, ~531KB) fused with a word unigram bonus.
//! The space symbol lets the model score word boundaries, which is where
//! short cryptograms are won or lost. No external model files.
//!
//! Driver: pattern-dictionary seed + random-restart simulated annealing +
//! greedy polish, then N-best re-ranking (word bigram + char trigram) and a
//! bigram-guided word polish. Single-letter words get an A/I pinning pass.

use crate::bigram_polish::bigram_polish;
use crate::codec::{Puzzle, Slot};
use crate::ngrams::{quad_index, quad_table};
use crate::patdict::{pattern_seed, Rng};
use crate::rerank::rerank_best;
use crate::words::{score_word, word_data, WordData};

#[derive(Clone, Debug)]
pub struct Solution {
    pub key: [u8; 26],
    pub score: f64,
}

#[derive(Clone, Debug)]
pub struct Config {
    // Simulated annealing driver.
    pub restarts: usize,
    pub steps: usize,
    pub t0: f64,
    pub t_end: f64,
    // Legacy / compatibility knobs (kept for CLI + WASM API stability).
    pub chi: f64,
    pub use_beam: bool,
    pub beam_width: usize,
    pub beam_nbest: usize,
    pub pin_single_ai: bool,
    // Dictionary-pattern seeds.
    pub dict_nodes: u64,
    pub dict_top_k: usize,
    pub dict_cand_cap: usize,
    pub use_pattern_seed: bool,
    pub seed: u64,
    // Scorer weights.
    pub word_w: f64,
    pub bigram_w: f64,
    pub trigram_w: f64,
    pub name_w: f64,
    pub use_correction: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            restarts: 48,
            steps: 12_000,
            t0: 10.0,
            t_end: 0.02,
            chi: 0.5,
            use_beam: false, // SA + re-rank is the robust path
            beam_width: 5000,
            beam_nbest: 12,
            pin_single_ai: true,
            dict_nodes: 200_000,
            dict_top_k: 6,
            dict_cand_cap: 2000,
            use_pattern_seed: true,
            seed: 0x9E37_79B9_7F4A_7C15,
            word_w: 3.0,
            bigram_w: 2.0,
            trigram_w: 1.0,
            name_w: 10.0,
            use_correction: false,
        }
    }
}

/// Clue locks: (locked_c[c], lock_plain[c]).
pub fn build_locks(clues: &[(u8, u8)]) -> ([bool; 26], [u8; 26]) {
    let mut locked_c = [false; 26];
    let mut lock_plain = [0u8; 26];
    for &(c, p) in clues {
        locked_c[c as usize] = true;
        lock_plain[c as usize] = p;
    }
    (locked_c, lock_plain)
}

// ---------------------------------------------------------------------------
// Full scoring
// ---------------------------------------------------------------------------

/// Word unigram bonus of the whole puzzle under `key` (no word_w applied).
fn word_bonus(puz: &Puzzle, key: &[u8; 26], wd: &WordData) -> f64 {
    let mut s = 0.0f64;
    let mut tok = Vec::with_capacity(16);
    for w in &puz.words {
        tok.clear();
        for sl in &w.slots {
            match sl {
                Slot::Apos => tok.push(b'\''),
                Slot::Letter(c) => tok.push(b'A' + key[*c as usize]),
            }
        }
        s += score_word(wd, &tok) as f64;
    }
    s
}

/// Full score of a key: quadgram log-probs (27 symbols, space = 26) +
/// word_w * word unigram bonus.
pub fn score_key(puz: &Puzzle, key: &[u8; 26], word_w: f64) -> f64 {
    let quad = quad_table();
    let wd = word_data();
    let n = puz.text.len();
    let mut s = 0.0f64;
    if n >= 4 {
        // Map cipher symbols through the key; 26 (space) passes through.
        let mut a = if puz.text[0] == 26 { 26 } else { key[puz.text[0] as usize] };
        let mut b = if puz.text[1] == 26 { 26 } else { key[puz.text[1] as usize] };
        let mut c = if puz.text[2] == 26 { 26 } else { key[puz.text[2] as usize] };
        for i in 3..n {
            let d = if puz.text[i] == 26 {
                26
            } else {
                key[puz.text[i] as usize]
            };
            s += quad[quad_index(a, b, c, d)] as f64;
            a = b;
            b = c;
            c = d;
        }
    }
    s + word_w * word_bonus(puz, key, wd)
}

// ---------------------------------------------------------------------------
// Incremental annealer
// ---------------------------------------------------------------------------

struct Searcher<'a> {
    puz: &'a Puzzle,
    quad: &'static [f32],
    wd: &'static WordData,
    word_w: f64,
    key: [u8; 26],
    #[allow(dead_code)]
    locked_c: [bool; 26],
    /// Unlocked cipher letters (swap candidates).
    free: Vec<u8>,
    dec: Vec<u8>,      // decoded symbols over puz.text (0..26 letters, 26 = space)
    qsum: f64,         // quadgram sum over dec
    wscores: Vec<f32>, // per-word unigram scores (no word_w)
    wsum: f64,
    // Scratch for delta_swap / apply_swap.
    qstarts: Vec<u32>,
    aff_words: Vec<u32>,
}

impl<'a> Searcher<'a> {
    fn new(puz: &'a Puzzle, word_w: f64, locked_c: [bool; 26]) -> Self {
        let free: Vec<u8> = (0..26u8).filter(|&c| !locked_c[c as usize]).collect();
        Searcher {
            puz,
            quad: quad_table(),
            wd: word_data(),
            word_w,
            key: [0u8; 26],
            locked_c,
            free,
            dec: Vec::new(),
            qsum: 0.0,
            wscores: Vec::new(),
            wsum: 0.0,
            qstarts: Vec::new(),
            aff_words: Vec::new(),
        }
    }

    fn total(&self) -> f64 {
        self.qsum + self.word_w * self.wsum
    }

    fn set_key(&mut self, key: [u8; 26]) {
        self.key = key;
        let n = self.puz.text.len();
        self.dec.clear();
        self.dec
            .extend(self.puz.text.iter().map(|&c| if c == 26 { 26 } else { key[c as usize] }));
        self.qsum = 0.0;
        if n >= 4 {
            for i in 0..n - 3 {
                self.qsum += self.quad[quad_index(
                    self.dec[i],
                    self.dec[i + 1],
                    self.dec[i + 2],
                    self.dec[i + 3],
                )] as f64;
            }
        }
        self.wscores.clear();
        self.wsum = 0.0;
        let mut tok = Vec::with_capacity(16);
        for w in &self.puz.words {
            tok.clear();
            for sl in &w.slots {
                match sl {
                    Slot::Apos => tok.push(b'\''),
                    Slot::Letter(c) => tok.push(b'A' + key[*c as usize]),
                }
            }
            let s = score_word(self.wd, &tok);
            self.wscores.push(s);
            self.wsum += s as f64;
        }
    }

    /// Collect affected quadgram window starts and word indices for a swap.
    fn collect(&mut self, x: u8, y: u8) {
        self.qstarts.clear();
        self.aff_words.clear();
        let n = self.dec.len() as i64;
        if n >= 4 {
            for c in [x, y] {
                for &p in &self.puz.pos_of_letter[c as usize] {
                    let p = p as i64;
                    for s in (p - 3).max(0)..=(p.min(n - 4)) {
                        self.qstarts.push(s as u32);
                    }
                }
            }
            self.qstarts.sort_unstable();
            self.qstarts.dedup();
        }
        for c in [x, y] {
            self.aff_words
                .extend_from_slice(&self.puz.words_of_letter[c as usize]);
        }
        self.aff_words.sort_unstable();
        self.aff_words.dedup();
    }

    /// Score delta of swapping cipher letters x and y (x,y unlocked).
    /// Returns (total_delta, quad_delta); the quad delta is passed to
    /// apply_swap so the incremental quad sum stays exact.
    fn delta_swap(&mut self, x: u8, y: u8) -> (f64, f64) {
        let kx = self.key[x as usize];
        let ky = self.key[y as usize];
        self.collect(x, y);
        let mut dq = 0.0f64;
        for &s in &self.qstarts {
            let s = s as usize;
            let old = self.quad[quad_index(
                self.dec[s],
                self.dec[s + 1],
                self.dec[s + 2],
                self.dec[s + 3],
            )] as f64;
            let mut v = [0u8; 4];
            for k in 0..4 {
                let d = self.dec[s + k];
                v[k] = if d == kx {
                    ky
                } else if d == ky {
                    kx
                } else {
                    d
                };
            }
            let new = self.quad[quad_index(v[0], v[1], v[2], v[3])] as f64;
            dq += new - old;
        }
        let mut dw = 0.0f64;
        let mut tok = Vec::with_capacity(16);
        for &wi in &self.aff_words {
            let w = &self.puz.words[wi as usize];
            tok.clear();
            for sl in &w.slots {
                match sl {
                    Slot::Apos => tok.push(b'\''),
                    Slot::Letter(c) => {
                        let cc = *c;
                        let p = if cc == x {
                            ky
                        } else if cc == y {
                            kx
                        } else {
                            self.key[cc as usize]
                        };
                        tok.push(b'A' + p);
                    }
                }
            }
            let new = score_word(self.wd, &tok) as f64;
            dw += new - self.wscores[wi as usize] as f64;
        }
        (dq + self.word_w * dw, dq)
    }

    fn apply_swap(&mut self, x: u8, y: u8, dq: f64) {
        self.key.swap(x as usize, y as usize);
        for &p in &self.puz.pos_of_letter[x as usize] {
            self.dec[p as usize] = self.key[x as usize];
        }
        for &p in &self.puz.pos_of_letter[y as usize] {
            self.dec[p as usize] = self.key[y as usize];
        }
        self.qsum += dq;
        // Refresh the affected-word list for THIS pair: callers (e.g. the
        // greedy polish scan) may have run other delta_swap calls since the
        // delta was computed, which would leave stale aff_words.
        self.collect(x, y);
        let mut tok = Vec::with_capacity(16);
        for &wi in &self.aff_words {
            let w = &self.puz.words[wi as usize];
            tok.clear();
            for sl in &w.slots {
                match sl {
                    Slot::Apos => tok.push(b'\''),
                    Slot::Letter(c) => tok.push(b'A' + self.key[*c as usize]),
                }
            }
            let new = score_word(self.wd, &tok);
            self.wsum += (new - self.wscores[wi as usize]) as f64;
            self.wscores[wi as usize] = new;
        }
    }

    fn anneal(&mut self, rng: &mut Rng, steps: usize, t0: f64, t_end: f64) {
        let nf = self.free.len();
        if nf < 2 || steps == 0 {
            return;
        }
        let cool = (t_end / t0).powf(1.0 / steps as f64);
        let mut t = t0;
        for _ in 0..steps {
            let i = rng.below(nf as u32) as usize;
            let mut j = rng.below((nf - 1) as u32) as usize;
            if j >= i {
                j += 1;
            }
            let x = self.free[i];
            let y = self.free[j];
            let (d, dq) = self.delta_swap(x, y);
            if d > 0.0 || (rng.next() as f64 / u64::MAX as f64) < (d / t).exp() {
                self.apply_swap(x, y, dq);
            }
            t *= cool;
        }
    }

    /// Greedy hill-climb over swaps until no single swap improves.
    fn polish(&mut self) {
        let nf = self.free.len();
        if nf < 2 {
            return;
        }
        // TEMP DEBUG: iteration cap to diagnose suspected non-termination.
        let mut iters = 0u32;
        loop {
            iters += 1;
            if iters > 200_000 {
                // Safety valve: with exact deltas this is unreachable in
                // practice; it only guards against future drift bugs hanging
                // the solver.
                break;
            }
            let mut best_d = 0.0f64;
            let mut best = None::<(u8, u8, f64)>;
            for ii in 0..nf {
                for jj in ii + 1..nf {
                    let (d, dq) = self.delta_swap(self.free[ii], self.free[jj]);
                    if d > best_d {
                        best_d = d;
                        best = Some((self.free[ii], self.free[jj], dq));
                    }
                }
            }
            match best {
                Some((x, y, dq)) => self.apply_swap(x, y, dq),
                None => break,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Key initialization
// ---------------------------------------------------------------------------

fn random_key(locked_c: &[bool; 26], lock_plain: &[u8; 26], rng: &mut Rng) -> [u8; 26] {
    let mut key = [0u8; 26];
    let mut used = [false; 26];
    for c in 0..26 {
        if locked_c[c] {
            key[c] = lock_plain[c];
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
        if !locked_c[c] {
            key[c] = free[fi];
            fi += 1;
        }
    }
    key
}

fn run_once(
    puz: &Puzzle,
    key: [u8; 26],
    locked_c: &[bool; 26],
    cfg: &Config,
    rng: &mut Rng,
) -> Solution {
    let mut s = Searcher::new(puz, cfg.word_w, *locked_c);
    s.set_key(key);
    s.anneal(rng, cfg.steps, cfg.t0, cfg.t_end);
    s.polish();
    Solution {
        score: s.total(),
        key: s.key,
    }
}

// ---------------------------------------------------------------------------
// Single-letter A/I pinning
// ---------------------------------------------------------------------------

/// Single-letter cipher words are almost always A or I. Try both for the
/// most frequent single-letter cipher letters; keep the better key.
fn pin_single_ai(
    puz: &Puzzle,
    sol: &Solution,
    cfg: &Config,
    locked_c: &[bool; 26],
) -> Solution {
    let mut counts = [0u32; 26];
    for w in &puz.words {
        if w.slots.len() == 1 {
            if let Slot::Letter(c) = w.slots[0] {
                counts[c as usize] += 1;
            }
        }
    }
    let mut order: Vec<u8> = (0..26u8).filter(|&c| counts[c as usize] > 0).collect();
    order.sort_by(|&a, &b| counts[b as usize].cmp(&counts[a as usize]));
    let mut best = sol.clone();
    // Score pins under the same objective used for candidates (quadgram +
    // word unigram). best.score is on the rerank's combined scale
    // (quad+word+bigram+trigram+name), which is far more negative, so
    // comparing score_key candidates against it would apply almost any pin
    // and garble correct keys (e.g. cryptoquip 2026-01-21).
    let mut best_score = score_key(puz, &best.key, cfg.word_w);
    for &c in order.iter().take(4) {
        if locked_c[c as usize] {
            continue;
        }
        for p in [b'A', b'I'] {
            let pv = p - b'A';
            if best.key[c as usize] == pv {
                continue;
            }
            // Find who currently maps to pv; skip if locked.
            let d = match (0..26u8).find(|&d| best.key[d as usize] == pv) {
                Some(d) => d,
                None => continue,
            };
            if locked_c[d as usize] {
                continue;
            }
            let mut key = best.key;
            key.swap(c as usize, d as usize);
            let s = score_key(puz, &key, cfg.word_w);
            if s > best_score {
                best_score = s;
                best = Solution { key, score: s };
            }
        }
    }
    best
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// N-best restart winners (multi-result), best first.
pub fn solve_candidates(puz: &Puzzle, clues: &[(u8, u8)], cfg: &Config) -> Vec<Solution> {
    let (locked_c, lock_plain) = build_locks(clues);
    let mut rng = Rng(cfg.seed);
    let mut winners: Vec<Solution> = Vec::new();
    if cfg.use_pattern_seed {
        if let Some(k) = pattern_seed(puz, &locked_c, &lock_plain, cfg.dict_nodes, &mut rng) {
            winners.push(run_once(puz, k, &locked_c, cfg, &mut rng));
        }
    }
    for _ in 0..cfg.restarts {
        let k = random_key(&locked_c, &lock_plain, &mut rng);
        winners.push(run_once(puz, k, &locked_c, cfg, &mut rng));
    }
    winners.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    winners.dedup_by(|a, b| a.key == b.key);
    winners
}

pub fn solve(puz: &Puzzle, clues: &[(u8, u8)], cfg: &Config) -> Solution {
    let (locked_c, _) = build_locks(clues);
    let winners = solve_candidates(puz, clues, cfg);
    assert!(!winners.is_empty());
    let mut best = rerank_best(puz, &winners, cfg);
    if cfg.pin_single_ai {
        best = pin_single_ai(puz, &best, cfg, &locked_c);
    }
    // Bigram-guided word polish (native only; WASM sets bigram_w = 0).
    bigram_polish(puz, &best, cfg, &locked_c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::parse;

    #[test]
    fn delta_matches_full_score() {
        let puz = parse("XPJ XLCSUH XLYX VJEOH RJ CV XLWB'FW HWPCSU XLCSUH PLCEW XLWB'FW PYEOCSU: SWWREW YSR XFWYR.");
        let mut rng = Rng(12345);
        let mut s = Searcher::new(&puz, 1.5, [false; 26]);
        let k = random_key(&[false; 26], &[0; 26], &mut rng);
        s.set_key(k);
        let before = s.total();
        let (d, dq) = s.delta_swap(3, 7);
        s.apply_swap(3, 7, dq);
        let after = s.total();
        let full = score_key(&puz, &s.key, 1.5);
        assert!(
            (after - before - d).abs() < 1e-3,
            "delta mismatch: {after} {before} {d}"
        );
        assert!(
            (after - full).abs() < 1e-3,
            "incremental drift: {after} {full}"
        );
    }

    #[test]
    fn solves_cryptoquip() {
        let puz = parse("XPJ XLCSUH XLYX VJEOH RJ CV XLWB'FW HWPCSU XLCSUH PLCEW XLWB'FW PYEOCSU: SWWREW YSR XFWYR.");
        let cfg = Config {
            restarts: 4,
            steps: 4000,
            ..Config::default()
        };
        let sol = solve(&puz, &[], &cfg);
        let text = crate::codec::replay(&puz.raw, &sol.key);
        assert!(
            text.contains("THEY'RE"),
            "expected THEY'RE in plaintext, got: {text}"
        );
    }
}
