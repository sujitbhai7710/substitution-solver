//! Diagnostic: is a miss a SEARCH error or a MODEL error?
//! Scores the solver's key and the true key under the EXACT final selection
//! objective (score_final). If truth > found, the search failed to find the
//! optimum (search error). If truth < found, the model prefers the wrong
//! answer (model error).
//! Usage: diag < puzzle.txt ; TRUE=<answer> in env.

use subst_solver::{exact_match, parse, replay, score_final, solve, Config};

fn main() {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).unwrap();
    let text = text.trim_end_matches('\n');
    let puz = parse(text);
    let cfg = Config::default();

    let sol = solve(&puz, &[], &cfg);
    let got = replay(text, &sol.key);
    let found_score = score_final(&puz, &sol.key, &cfg);

    let ans = std::env::var("TRUE").expect("TRUE env required");
    let mut key = [0u8; 26];
    let mut seen = [false; 26];
    for (pc, ac) in text.bytes().zip(ans.bytes()) {
        let pc = pc.to_ascii_uppercase();
        let ac = ac.to_ascii_uppercase();
        if pc.is_ascii_alphabetic() && ac.is_ascii_alphabetic() {
            let c = (pc - b'A') as usize;
            if !seen[c] {
                seen[c] = true;
                key[c] = ac - b'A';
            }
        }
    }
    let mut used = [false; 26];
    for c in 0..26 {
        if seen[c] {
            used[key[c] as usize] = true;
        }
    }
    let free: Vec<u8> = (0..26u8).filter(|p| !used[*p as usize]).collect();
    let mut fi = 0;
    for c in 0..26 {
        if !seen[c] {
            key[c] = free[fi];
            fi += 1;
        }
    }
    let truth = replay(text, &key);
    let truth_score = score_final(&puz, &key, &cfg);

    println!("got:         {}", got);
    println!("truth:       {}", truth);
    println!("found_score: {:.1}", found_score);
    println!("truth_score: {:.1}", truth_score);
    println!(
        "verdict: {}",
        if truth_score > found_score + 1e-6 {
            "SEARCH ERROR (truth scores higher)"
        } else if found_score > truth_score + 1e-6 {
            "MODEL ERROR (model prefers wrong answer)"
        } else {
            "TIE"
        }
    );
    println!("exact_match(truth): {}", exact_match(&truth, &ans));
}
