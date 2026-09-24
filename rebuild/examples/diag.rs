//! Failure-analysis diagnostic: solve a puzzle exactly like solve_one, then
//! score the found key and the true key under the final selection objective
//! (full_score). Prints one JSON line classifying the outcome.
//!
//! Usage: echo "$puzzle" | TRUE="$answer" diag
//! Env overrides mirror solve_one.rs (R, S, T0, TE, WW, BW, NOBEAM, BG, NOPAT).

use subst_solver::{full_score, parse, parse_clue, replay, solve, Config};

fn env_usize(name: &str, dflt: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(dflt)
}
fn env_f64(name: &str, dflt: f64) -> f64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(dflt)
}
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}
fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_alphabetic() {
            cur.push(c.to_ascii_uppercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    out
}
fn jstr(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn main() {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).unwrap();
    let text = text.trim_end_matches('\n');
    let ans = std::env::var("TRUE").expect("TRUE env var required");

    let puz = parse(text);
    // Mirror solve_one.rs exactly.
    let mut cfg = Config::default();
    // Defaults mirror Config::default() (production/WASM); env only overrides.
    cfg.restarts = env_usize("R", cfg.restarts);
    cfg.steps = env_usize("S", cfg.steps);
    cfg.t0 = env_f64("T0", cfg.t0);
    cfg.t_end = env_f64("TE", cfg.t_end);
    cfg.word_w = env_f64("WW", cfg.word_w);
    cfg.beam_width = env_usize("BW", cfg.beam_width);
    cfg.use_beam = std::env::var("BEAM").is_ok();
    cfg.bigram_w = env_f64("BG", cfg.bigram_w);
    cfg.trigram_w = env_f64("TW", cfg.trigram_w);
    cfg.name_w = env_f64("NW", cfg.name_w);
    cfg.use_pattern_seed = std::env::var("NOPAT").is_err();
    cfg.seed = 0x1234_5678_9abc_def0;

    let sol = solve(&puz, &parse_clue(text), &cfg);
    let found_text = replay(text, &sol.key);
    let ok = norm(&found_text) == norm(&ans);

    // Derive the true key from letter-to-letter alignment (robust to
    // punctuation/encoding differences between puzzle and answer).
    let pc: Vec<u8> = text.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
    let ac: Vec<u8> = ans.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
    let mut key = [0u8; 26];
    let mut seen = [false; 26];
    for (&pb, &ab) in pc.iter().zip(ac.iter()) {
        let c = (pb.to_ascii_uppercase() - b'A') as usize;
        if !seen[c] {
            seen[c] = true;
            key[c] = ab.to_ascii_uppercase() - b'A';
        }
    }
    let true_text = replay(text, &key);

    let found_score = full_score(&puz, &sol.key, &cfg);
    let true_score = full_score(&puz, &key, &cfg);
    let category = if ok {
        "OK"
    } else if true_score > found_score + 1e-6 {
        "SEARCH_MISS"
    } else {
        "MODEL_PREFERS_WRONG"
    };

    // Word-level diffs between found and true decodes.
    let fw = words(&found_text);
    let tw = words(&true_text);
    let mut diffs = Vec::new();
    for (a, b) in fw.iter().zip(tw.iter()) {
        if a != b {
            diffs.push(format!("{a}->{b}"));
            if diffs.len() >= 12 {
                break;
            }
        }
    }

    println!(
        "{{\"ok\":{ok},\"category\":{cat},\"found_score\":{fs:.2},\"true_score\":{ts:.2},\"delta\":{d:.2},\"n_diff_words\":{n},\"diffs\":[{diffs}],\"found_text\":{ft},\"true_text\":{tt}}}",
        ok = ok,
        cat = jstr(category),
        fs = found_score,
        ts = true_score,
        d = true_score - found_score,
        n = fw.iter().zip(tw.iter()).filter(|(a, b)| a != b).count(),
        diffs = diffs.iter().map(|d| jstr(d)).collect::<Vec<_>>().join(","),
        ft = jstr(&found_text),
        tt = jstr(&true_text),
    );
}
