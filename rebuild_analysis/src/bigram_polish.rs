//! Bigram-guided word-level polish.
//!
//! After the main search, we do a hill-climb over word replacements:
//! for each puzzle word, try pattern-dictionary candidates and accept
//! the one that maximizes (quadgram + word + bigram). This fixes cases
//! like LIKE->LIFE where the bigram "LIFE WILL" beats "LIKE WILL".

use crate::bigrams::bigrams;
use crate::codec::{replay, Puzzle, Slot};
use crate::ngrams::{quad_index, quad_table};
use crate::search::{score_key, Config, Solution};
use crate::words::word_data;

/// Bigram-guided word polish: hill-climb on word replacements.
/// `locked_c` marks cipher letters with clue-fixed mappings; any candidate
/// move that would remap a locked letter is rejected outright.
/// Returns an improved Solution (or the original if no improvement).
pub fn bigram_polish(
    puz: &Puzzle,
    sol: &Solution,
    cfg: &Config,
    locked_c: &[bool; 26],
) -> Solution {
    if cfg.bigram_w == 0.0 {
        return sol.clone();
    }
    let wd = word_data();
    let mut key = sol.key;
    let mut best_score = full_score(puz, &key, cfg);

    // Decode the current key to get word strings for bigram context.
    // We'll recompute as needed.

    for _iter in 0..3 {
        let mut improved = false;
        // Decode current plaintext words for neighbor context.
        let cur_text = replay(&puz.raw, &key);
        let cur_words: Vec<Vec<u8>> = split_words(cur_text.as_bytes());
        for wi in 0..puz.words.len() {
            let w = &puz.words[wi];
            // Compute cipher pattern (same as cipher_pattern_into in search.rs).
            let mut pat = Vec::with_capacity(w.slots.len());
            let mut pmap = [-1i8; 26];
            let mut pnext = 0u8;
            let mut has_apos = false;
            for s in &w.slots {
                match s {
                    Slot::Apos => {
                        pat.push(b'\'');
                        has_apos = true;
                    }
                    Slot::Letter(c) => {
                        let c = *c as usize;
                        if pmap[c] < 0 {
                            pmap[c] = pnext as i8;
                            pnext += 1;
                        }
                        pat.push(b'A' + pmap[c] as u8);
                    }
                }
            }
            if has_apos {
                continue;
            }
            let cands = match wd.patterns.get(&pat) {
                Some(v) => v,
                None => continue,
            };
            // Collect cipher letters in order.
            let ciph: Vec<usize> = w
                .slots
                .iter()
                .filter_map(|s| match s {
                    Slot::Letter(c) => Some(*c as usize),
                    Slot::Apos => None,
                })
                .collect();
            // Get neighbor words for context.
            let prev_w = if wi > 0 {
                Some(cur_words[wi - 1].clone())
            } else {
                None
            };
            let next_w = if wi + 1 < cur_words.len() {
                Some(cur_words[wi + 1].clone())
            } else {
                None
            };
            // Score candidates by quadgram log-prob of the neighbor+word+
            // neighbor letter stream, joined with the space symbol (26) so
            // word boundaries score properly under the 27-symbol model.
            // Take top 40.
            let quads = quad_table();
            let mut scored: Vec<(f32, u32)> = Vec::with_capacity(cands.len().min(500));
            for &ci in cands.iter().take(500) {
                let cw = &wd.words[ci as usize];
                if cw.len() != ciph.len() {
                    continue;
                }
                let mut phrase = Vec::with_capacity(40);
                if let Some(ref pw) = prev_w {
                    phrase.extend_from_slice(pw);
                    phrase.push(26);
                }
                phrase.extend_from_slice(cw);
                if next_w.is_some() {
                    phrase.push(26);
                }
                if let Some(ref nw) = next_w {
                    phrase.extend_from_slice(nw);
                }
                let mut s = 0.0f32;
                if phrase.len() >= 4 {
                    // Phrase holds ASCII bytes; 26 is the space symbol.
                    let sym = |x: u8| if x == 26 { 26 } else { x - b'A' };
                    for k in 0..phrase.len() - 3 {
                        s += quads[quad_index(
                            sym(phrase[k]),
                            sym(phrase[k + 1]),
                            sym(phrase[k + 2]),
                            sym(phrase[k + 3]),
                        )];
                    }
                }
                scored.push((s, ci));
            }
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            for &(_, ci) in scored.iter().take(40) {
                let cw = &wd.words[ci as usize];
                if cw.len() != ciph.len() {
                    continue;
                }
                // Build the swaps needed to map w -> cw.
                let mut tmpkey = key;
                let mut ok = true;
                let mut c2p = [0u8; 26];
                let mut p2c = [0u8; 26];
                for (i, &c) in ciph.iter().enumerate() {
                    let p = (cw[i] - b'A') as usize;
                    if c2p[c] == 0 && p2c[p] == 0 {
                        let cur_p = tmpkey[c] as usize;
                        if cur_p != p {
                            // Clue-locked letters must never be remapped.
                            if locked_c[c] {
                                ok = false;
                                break;
                            }
                            // Find c2 such that tmpkey[c2] == p, swap.
                            let mut c2 = 26;
                            for j in 0..26 {
                                if tmpkey[j] as usize == p {
                                    c2 = j;
                                    break;
                                }
                            }
                            if c2 == 26 || locked_c[c2] {
                                ok = false;
                                break;
                            }
                            tmpkey.swap(c, c2);
                        }
                    } else if c2p[c] as usize != p + 1 || p2c[p] as usize != c + 1 {
                        ok = false;
                        break;
                    }
                    c2p[c] = (p + 1) as u8;
                    p2c[p] = (c + 1) as u8;
                }
                if !ok {
                    continue;
                }
                let s = full_score(puz, &tmpkey, cfg);
                if s > best_score + 1e-9 {
                    best_score = s;
                    key = tmpkey;
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }

    Solution {
        key,
        score: best_score,
    }
}

/// Split text into uppercase word byte-vectors.
fn split_words(text: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    for &c in text.iter().chain(std::iter::once(&b' ')) {
        if c.is_ascii_alphabetic() {
            cur.push(c.to_ascii_uppercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    out
}

/// Full score: quadgram + word unigram (via score_key) + word bigram +
/// gazetteer full-name bonus. pub so diagnostics can score arbitrary keys
/// under the final objective.
pub fn full_score(puz: &Puzzle, key: &[u8; 26], cfg: &Config) -> f64 {
    let qw = score_key(puz, key, cfg.word_w);
    let text = replay(&puz.raw, key);
    let tb = text.as_bytes();
    let mut total = qw;
    if cfg.bigram_w != 0.0 {
        let bg = bigrams();
        let wd = word_data();
        let mut btotal = 0.0f32;
        let mut prev_id: Option<u32> = None;
        let mut cur = Vec::with_capacity(16);
        for &c in tb.iter().chain(std::iter::once(&b' ')) {
            if c.is_ascii_alphabetic() {
                cur.push(c.to_ascii_uppercase());
            } else if !cur.is_empty() {
                let id = wd.by_word.get(&cur).copied().unwrap_or(u32::MAX);
                if let Some(p) = prev_id {
                    btotal += bg.score_pair(p, id);
                }
                prev_id = Some(id);
                cur.clear();
            }
        }
        total += cfg.bigram_w * btotal as f64;
    }
    if cfg.name_w != 0.0 {
        total += cfg.name_w * crate::gazetteer::name_bonus(tb) as f64;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{parse, parse_clue};
    use crate::search::solve;

    /// Regression test: clue-locked cipher->plain mappings must survive the
    /// entire solve pipeline, including bigram_polish.
    /// Puzzle decodes to "TWO THINGS ... THEY'RE ...", so W->E is the true map.
    #[test]
    fn clue_locks_survive_full_solve() {
        let puzzle = "XPJ XLCSUH XLYX VJEOH RJ CV XLWB'FW HWPCSU XLCSUH PLCEW XLWB'FW PYEOCSU: SWWREW YSR XFWYR.";
        let puz = parse(puzzle);
        let clues = parse_clue("W=E");
        let mut cfg = Config::default();
        cfg.restarts = 6;
        cfg.steps = 3000;
        let sol = solve(&puz, &clues, &cfg);
        assert_eq!(
            sol.key[(b'W' - b'A') as usize],
            b'E' - b'A',
            "clue lock W->E was violated by the solver"
        );
    }

    /// Direct unit test: bigram_polish must never remap a locked cipher letter,
    /// even when the lock contradicts the language model.
    #[test]
    fn bigram_polish_never_moves_locked_letters() {
        let puzzle = "XPJ XLCSUH XLYX VJEOH RJ CV XLWB'FW HWPCSU XLCSUH PLCEW XLWB'FW PYEOCSU: SWWREW YSR XFWYR.";
        let puz = parse(puzzle);
        // Deliberately wrong lock: X actually decodes to T.
        let mut locked_c = [false; 26];
        locked_c[(b'X' - b'A') as usize] = true;
        // Key honoring the lock (X->Q, rest identity-ish but bijective).
        let mut key = [0u8; 26];
        for c in 0..26 {
            key[c] = c as u8;
        }
        key[(b'X' - b'A') as usize] = b'Q' - b'A';
        key[(b'Q' - b'A') as usize] = b'X' - b'A';
        let sol = Solution { key, score: 0.0 };
        let cfg = Config::default();
        let out = bigram_polish(&puz, &sol, &cfg, &locked_c);
        assert_eq!(
            out.key[(b'X' - b'A') as usize],
            b'Q' - b'A',
            "bigram_polish remapped a clue-locked letter"
        );
    }
}
