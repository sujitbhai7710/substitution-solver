//! subst-solver: fast monoalphabetic substitution solver.
//!
//! Pipeline: canonicalize (letters + word breaks only; every non-letter is a
//! verbatim reconstruction record) -> solve (pattern-dictionary seed +
//! random-restart simulated annealing + greedy polish, scored by quadgram
//! log-probs fused with a word unigram bonus) -> replay (original
//! punctuation / casing / whitespace re-applied at exact positions).
//!
//! The solver NEVER lossy-normalizes input: curly quotes, em dashes and
//! friends pass through untouched, which is what makes exact-match output
//! possible.

mod bigram_polish;
mod bigrams;
mod codec;
mod correction;
mod gazetteer;
mod ngrams;
mod patdict;
mod rerank;
mod search;
mod trigrams;
mod words;
// Round-5 models (charlm/wordlm/models): removed in round 6; the solver is
// small-table only again (quadgrams + wordlist), all-WASM friendly.

pub use codec::{exact_match, exact_winnable, parse, parse_clue, replay, substitution_consistent, Puzzle};
pub use patdict::{dict_attack, DictHit};
pub use search::{build_locks, score_key, solve, solve_candidates, Config, Solution};
/// Final selection objective (quadgram + word_w * word unigram + bigram_w * word
/// bigram). Exposed for diagnostics: compare a found key vs the true key to
/// distinguish search errors from model errors.
pub use bigram_polish::full_score;

/// Score a key under the final selection objective (quadgram + word_w *
/// word unigram). Diagnostic use: compare a found key vs the true key to
/// distinguish search errors (truth scores higher) from model errors
/// (truth scores lower).
pub fn score_final(puz: &Puzzle, key: &[u8; 26], cfg: &Config) -> f64 {
    score_key(puz, key, cfg.word_w)
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

pub struct SolveResult {
    pub plaintext: String,
    pub key: String, // 26 chars: plain letter for cipher A..Z
    pub score: f64,
    pub ms: f64,
}

/// Warm the embedded tables (call once; not timed as part of solving).
pub fn warmup() {
    let _ = ngrams::quad_table();
    let _ = words::word_data();
    let _ = trigrams::trigrams();
}

pub fn solve_text(puzzle: &str, clue: &str, cfg: &Config) -> SolveResult {
    #[cfg(target_arch = "wasm32")]
    let t0: Option<()> = None; // Instant not available in WASM; skip timing.
    #[cfg(not(target_arch = "wasm32"))]
    let t0 = std::time::Instant::now();
    let puz = parse(puzzle);
    let clues = parse_clue(clue);
    let sol = solve(&puz, &clues, cfg);
    let plaintext = replay(&puz.raw, &sol.key);
    #[cfg(target_arch = "wasm32")]
    let ms = 0.0;
    #[cfg(not(target_arch = "wasm32"))]
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    let key: String = sol.key.iter().map(|&p| (b'A' + p) as char).collect();
    SolveResult {
        plaintext,
        key,
        score: sol.score,
        ms,
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn solve_js(puzzle: &str, clue: &str, restarts: u32, steps: u32) -> String {
    let cfg = Config {
        restarts: restarts as usize,
        steps: steps as usize,
        ..Config::default()
    };
    // WASM now ships the compact 60k bigram table (~300KB), so bigram
    // re-ranking stays enabled.
    let r = solve_text(puzzle, clue, &cfg);
    format!(
        "{{\"plaintext\":\"{}\",\"key\":\"{}\",\"score\":{:.2},\"ms\":{:.1}}}",
        json_escape(&r.plaintext),
        r.key,
        r.score,
        r.ms
    )
}

/// Explicit warmup export so the page can init models off the critical path.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn warmup_js() {
    warmup();
}

/// N-best results for the page: top-N distinct candidates re-ranked by the
/// combined final-selection score, best first. JSON array of
/// {"plaintext","key","score"}.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn solve_n_js(puzzle: &str, clue: &str, restarts: u32, steps: u32, n: u32) -> String {
    let cfg = Config {
        restarts: restarts as usize,
        steps: steps as usize,
        ..Config::default()
    };
    let puz = parse(puzzle);
    let clues = parse_clue(clue);
    let winners = solve_candidates(&puz, &clues, &cfg);
    assert!(!winners.is_empty());
    let ranked = crate::rerank::rerank_all(&puz, &winners, &cfg);
    let n = (n as usize).clamp(1, ranked.len());
    let mut out = String::from("[");
    for (i, s) in ranked.iter().take(n).enumerate() {
        let plaintext = replay(&puz.raw, &s.key);
        let key: String = s.key.iter().map(|&p| (b'A' + p) as char).collect();
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"plaintext\":\"{}\",\"key\":\"{}\",\"score\":{:.2}}}",
            json_escape(&plaintext),
            key,
            s.score
        ));
    }
    out.push(']');
    out
}
