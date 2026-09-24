//! Decompose full_score into components for truth vs got.
//! Usage: decompdbg < puzzle.txt ; TRUE=<answer> in env.
use subst_solver::{full_score, parse, score_key, Config};

fn main() {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).unwrap();
    let text = text.trim_end_matches('\n');
    let puz = parse(text);
    let ans = std::env::var("TRUE").expect("TRUE env required");
    let cfg = Config::default();

    // Build true key
    let pc: Vec<u8> = text.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
    let ac: Vec<u8> = ans.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
    let mut tk = [0u8; 26];
    let mut seen = [false; 26];
    for (&pb, &ab) in pc.iter().zip(ac.iter()) {
        let c = (pb.to_ascii_uppercase() - b'A') as usize;
        let p = (ab.to_ascii_uppercase() - b'A') as usize;
        if !seen[c] { seen[c] = true; tk[c] = p as u8; }
    }
    let mut used = [false; 26];
    for c in 0..26 { if seen[c] { used[tk[c] as usize] = true; } }
    let free: Vec<u8> = (0..26u8).filter(|p| !used[*p as usize]).collect();
    let mut fi = 0;
    for c in 0..26 { if !seen[c] { tk[c] = free[fi]; fi += 1; } }

    // Solver's key
    let sol = subst_solver::solve(&puz, &[], &cfg);
    let gk = sol.key;

    // Decompose: quad, word, bigram, trigram, name
    for (name, k) in [("truth", tk), ("got", gk)] {
        let qw = score_key(&puz, &k, cfg.word_w);
        let q0 = score_key(&puz, &k, 0.0);
        let word_part = qw - q0;
        let full = full_score(&puz, &k, &cfg);
        // bigram+trigram+name = full - qw
        let rest = full - qw;
        println!("{}: quad={:.1} word3x={:.1} qw={:.1} btg+name={:.1} full={:.1}",
                 name, q0, word_part, qw, rest, full);
    }
    let _ = cfg;
}
