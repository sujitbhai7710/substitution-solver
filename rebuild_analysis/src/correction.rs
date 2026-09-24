//! Final dictionary/gazetteer correction pass.
//!
//! Fixes residual word errors that survive the main search and polish:
//! - Out-of-vocabulary tokens (proper nouns, rare words) via
//!   pattern-matched dictionary words and gazetteer name parts.
//! - Very-low-frequency dictionary words (FIG vs FIX) via higher-frequency
//!   pattern-equivalent alternatives.
//!
//! Design (per the last-percent accuracy research):
//! 1. Compute per-word suspicion (OOV or near-OOV frequency).
//! 2. Generate pattern-equivalent candidates (dict + gazetteer).
//! 3. Reject candidates that conflict with clue locks.
//! 4. Accept only when the full-text combined score
//!    (quadgram + word-unigram + char-trigram) improves.
//!
//! WASM-compatible: uses no bigram table. Runs after bigram_polish in
//! `search::solve`, so it improves both native and WASM.

use crate::codec::{replay, Puzzle, Slot};
use crate::gazetteer::candidates_for_pattern as gazetteer_candidates;
use crate::ngrams::{quad_index, quad_table};
use crate::search::{Config, Solution};
use crate::trigrams::trigrams;
use crate::words::{score_word, word_data};

/// How far above the OOV penalty a word's log-prob can be and still count
/// as "suspiciously rare". Very rare dictionary words (FIG) are nearly as
/// surprising as OOV, so they get correction candidates too.
/// (Currently unused; the correction is gazetteer-only for safety.)
#[allow(dead_code)]
const RARE_MARGIN: f32 = 0.5;

/// Max dictionary pattern candidates tried per suspicious word.
/// (Currently unused; the correction is gazetteer-only for safety.)
#[allow(dead_code)]
const MAX_DICT_CANDS: usize = 80;

/// Max correction iterations over the word list.
const MAX_ITERS: usize = 1;

/// Minimum score improvement required to accept a correction.
/// Must be large to avoid breaking correct solutions (the model is imperfect).
/// The cryptoquip 2026-01-21 regression (correct solution garbled) showed that
/// even a 3.0 threshold is insufficient; use 5.0 for dictionary candidates.
const MIN_IMPROVEMENT: f64 = 5.0;

/// Gazetteer (proper noun) candidates are higher-confidence: a matching name
/// with the same pattern is strong evidence. Use a lenient threshold.
/// (JENDAYA -> ZENDAYA needs this; the word model doesn't know either name.)
const GAZETTEER_IMPROVEMENT: f64 = 0.5;

/// Lightweight full-text score: quadgram + word-unigram + char-trigram.
/// No Searcher construction, no bigram table (WASM-safe).
fn light_score(puz: &Puzzle, key: &[u8; 26], cfg: &Config) -> f64 {
    let wd = word_data();
    let quads = quad_table();

    // Decode the text stream (27 symbols; 26 = space) for quadgrams.
    let mut plain: Vec<u8> = Vec::with_capacity(puz.text.len());
    for &c in &puz.text {
        plain.push(if c == 26 { 26 } else { key[c as usize] }); // 0..27 values
    }
    let mut qsum = 0.0f64;
    if plain.len() >= 4 {
        for i in 0..plain.len() - 3 {
            qsum += quads[quad_index(plain[i], plain[i + 1], plain[i + 2], plain[i + 3])] as f64;
        }
    }

    // Word-unigram score.
    let mut wsum = 0.0f64;
    let mut wbuf: Vec<u8> = Vec::with_capacity(16);
    for w in &puz.words {
        wbuf.clear();
        for s in &w.slots {
            match s {
                Slot::Apos => wbuf.push(b'\''),
                Slot::Letter(c) => wbuf.push(b'A' + key[*c as usize]),
            }
        }
        wsum += score_word(wd, &wbuf) as f64;
    }

    // Char-trigram score on the replayed text (sees word boundaries).
    let text = replay(&puz.raw, key);
    let tgsum = trigrams().score_bytes(text.as_bytes()) as f64;

    qsum + cfg.word_w * wsum + cfg.trigram_w * tgsum
}

/// Compute the cipher letter-pattern of a word's slots.
/// (Same canonical form as `words::pattern_of`, for pattern-dict lookup.)
fn cipher_pattern(w: &crate::codec::WordTok, has_apos: &mut bool) -> Vec<u8> {
    let mut pat = Vec::with_capacity(w.slots.len());
    let mut pmap = [-1i8; 26];
    let mut pnext = 0u8;
    *has_apos = false;
    for s in &w.slots {
        match s {
            Slot::Apos => {
                pat.push(b'\'');
                *has_apos = true;
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
    pat
}

/// Try to remap `key` so the cipher word `ciph` decodes to `cand`.
/// Returns the new key on success, or None if the remap would touch a
/// clue-locked cipher letter or break bijectivity.
/// `ciph`: cipher letters (0..26) in word order. `cand`: uppercase plain bytes.
fn remap_for_candidate(
    key: &[u8; 26],
    ciph: &[usize],
    cand: &[u8],
    locked_c: &[bool; 26],
) -> Option<[u8; 26]> {
    if ciph.len() != cand.len() {
        return None;
    }
    let mut tmpkey = *key;
    // Verify pattern consistency and build swaps.
    let mut c2p = [-1i8; 26];
    let mut p2c = [-1i8; 26];
    for (i, &c) in ciph.iter().enumerate() {
        let p = (cand[i] - b'A') as usize;
        if c2p[c] == -1 && p2c[p] == -1 {
            let cur_p = tmpkey[c] as usize;
            if cur_p != p {
                if locked_c[c] {
                    return None;
                }
                // Find c2 with tmpkey[c2] == p; swap c <-> c2.
                let mut c2 = 26usize;
                for j in 0..26 {
                    if tmpkey[j] as usize == p {
                        c2 = j;
                        break;
                    }
                }
                if c2 == 26 || locked_c[c2] {
                    return None;
                }
                tmpkey.swap(c, c2);
            }
            c2p[c] = p as i8;
            p2c[p] = c as i8;
        } else if c2p[c] != p as i8 || p2c[p] != c as i8 {
            return None;
        }
    }
    Some(tmpkey)
}

/// Final correction pass. Returns an improved Solution (or the original).
pub fn dictionary_correct(
    puz: &Puzzle,
    sol: &Solution,
    cfg: &Config,
    locked_c: &[bool; 26],
) -> Solution {
    let wd = word_data();
    let mut key = sol.key;
    let mut best_score = light_score(puz, &key, cfg);

    // Precompute cipher patterns and cipher-letter sequences per word.
    struct WInfo {
        pat: Vec<u8>,
        ciph: Vec<usize>,
        has_apos: bool,
    }
    let winfos: Vec<WInfo> = puz
        .words
        .iter()
        .map(|w| {
            let mut has_apos = false;
            let pat = cipher_pattern(w, &mut has_apos);
            let ciph: Vec<usize> = w
                .slots
                .iter()
                .filter_map(|s| match s {
                    Slot::Letter(c) => Some(*c as usize),
                    Slot::Apos => None,
                })
                .collect();
            WInfo {
                pat,
                ciph,
                has_apos,
            }
        })
        .collect();

    let mut wbuf: Vec<u8> = Vec::with_capacity(16);

    for _iter in 0..MAX_ITERS {
        let mut improved = false;

        for (wi, winfo) in winfos.iter().enumerate() {
            // Decode current word.
            wbuf.clear();
            for s in &puz.words[wi].slots {
                match s {
                    Slot::Apos => wbuf.push(b'\''),
                    Slot::Letter(c) => wbuf.push(b'A' + key[*c as usize]),
                }
            }

            // Word log-prob and OOV status.
            let (in_dict, logp) = match wd.by_word.get(&wbuf) {
                Some(&i) => (true, wd.logp[i as usize]),
                None => {
                    // Possessive fallback (same rule as score_word).
                    let n = wbuf.len();
                    if n > 2 && wbuf[n - 2] == b'\'' && wbuf[n - 1] == b'S' {
                        match wd.by_word.get(&wbuf[..n - 2]) {
                            Some(&i) => (true, wd.logp[i as usize] - 1.0),
                            None => (false, wd.oov),
                        }
                    } else {
                        (false, wd.oov)
                    }
                }
            };

            // Suspicion: OOV, or dictionary word so rare it's near-OOV.
            // (Common words are trusted; the main search already optimized them.)
            let suspicious = !in_dict || logp < wd.oov + RARE_MARGIN;
            if !suspicious {
                continue;
            }
            // Very short words (1-2 letters) are too ambiguous to correct;
            // the search already handles them via n-grams.
            if wbuf.len() < 3 {
                continue;
            }

            // Generate candidates: dict pattern-matches + gazetteer names.
            // (All share the cipher pattern by construction.)
            let mut best_cand: Option<Vec<u8>> = None;
            let mut best_cand_score = best_score;

            // 1. Dictionary pattern candidates (frequency order).
            if let Some(cands) = wd.patterns.get(&winfo.pat) {
                let mut tried = 0;
                for &ci in cands.iter() {
                    if tried >= MAX_DICT_CANDS {
                        break;
                    }
                    let cw = &wd.words[ci as usize];
                    if cw.len() != winfo.ciph.len() {
                        continue;
                    }
                    tried += 1;
                    // For rare-but-in-dict words, only consider strictly
                    // more frequent alternatives (avoid lateral moves).
                    if in_dict && wd.logp[ci as usize] <= logp + 1.0 {
                        continue;
                    }
                    // Skip the current decoding itself.
                    if cw == &wbuf {
                        continue;
                    }
                    if let Some(tk) = remap_for_candidate(&key, &winfo.ciph, cw, locked_c) {
                        let s = light_score(puz, &tk, cfg);
                        if s > best_cand_score + MIN_IMPROVEMENT {
                            best_cand_score = s;
                            best_cand = Some(cw.clone());
                        }
                    }
                }
            }

            // 2. Gazetteer name parts (OOV only; names aren't in the dict).
            // Skip words with apostrophes (names don't have them).
            if !in_dict && !winfo.has_apos {
                for gp in gazetteer_candidates(&winfo.pat) {
                    if gp.len() != winfo.ciph.len() {
                        continue;
                    }
                    if gp == &wbuf {
                        continue;
                    }
                    if let Some(tk) = remap_for_candidate(&key, &winfo.ciph, gp, locked_c) {
                        let s = light_score(puz, &tk, cfg);
                        // Gazetteer: lenient threshold (high-confidence names).
                        if s > best_cand_score + GAZETTEER_IMPROVEMENT {
                            best_cand_score = s;
                            best_cand = Some(gp.clone());
                        }
                    }
                }
            }

            // Apply the best candidate found for this word.
            if let Some(cand) = best_cand {
                if let Some(tk) = remap_for_candidate(&key, &winfo.ciph, &cand, locked_c) {
                    key = tk;
                    best_score = best_cand_score;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{parse, parse_clue};
    use crate::search::solve;

    /// Clue locks must survive the correction pass.
    #[test]
    fn correction_respects_clue_locks() {
        let puzzle = "XPJ XLCSUH XLYX VJEOH RJ CV XLWB'FW HWPCSU XLCSUH PLCEW XLWB'FW PYEOCSU: SWWREW YSR XFWYR.";
        let puz = parse(puzzle);
        let clues = parse_clue("W=E");
        let mut cfg = Config::default();
        cfg.restarts = 4;
        cfg.steps = 2000;
        let sol = solve(&puz, &clues, &cfg);
        let (locked_c, _) = crate::search::build_locks(&clues);
        let out = dictionary_correct(&puz, &sol, &cfg, &locked_c);
        assert_eq!(
            out.key[(b'W' - b'A') as usize],
            b'E' - b'A',
            "correction remapped a clue-locked letter"
        );
    }

    #[test]
    fn light_score_is_finite() {
        let puz = parse("HELLO WORLD");
        let key = core::array::from_fn(|i| i as u8);
        let cfg = Config::default();
        let s = light_score(&puz, &key, &cfg);
        assert!(s.is_finite(), "light_score not finite: {}", s);
    }
}
