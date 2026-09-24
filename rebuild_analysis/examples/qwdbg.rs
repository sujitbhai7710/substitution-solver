//! Decompose score_key into quadgram vs word-unigram parts for truth vs got.
//! Usage: qwdbg < puzzle.txt ; TRUE=<answer> in env.
use subst_solver::{parse, replay, score_key, solve, Config};

fn main() {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).unwrap();
    let text = text.trim_end_matches('\n');
    let puz = parse(text);
    let ans = std::env::var("TRUE").expect("TRUE env required");
    let pc: Vec<u8> = text.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
    let ac: Vec<u8> = ans.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
    assert_eq!(pc.len(), ac.len());
    let mut tk = [0u8; 26];
    let mut seen = [false; 26];
    let mut seenp = [false; 26];
    for (&pb, &ab) in pc.iter().zip(ac.iter()) {
        let c = (pb.to_ascii_uppercase() - b'A') as usize;
        let p = (ab.to_ascii_uppercase() - b'A') as usize;
        if !seen[c] {
            assert!(!seenp[p]);
            seen[c] = true; seenp[p] = true; tk[c] = p as u8;
        }
    }
    let mut used = [false; 26];
    for c in 0..26 { if seen[c] { used[tk[c] as usize] = true; } }
    let free: Vec<u8> = (0..26u8).filter(|p| !used[*p as usize]).collect();
    let mut fi = 0;
    for c in 0..26 { if !seen[c] { tk[c] = free[fi]; fi += 1; } }

    // got key: from the actual solver
    let cfg = Config::default();
    let sol = solve(&puz, &[], &cfg);
    let gkarr = sol.key;
    println!("got text: {}", &replay(text, &gkarr)[..100.min(text.len())]);

    for (name, k) in [("truth", tk), ("got", gkarr)] {
        let q = score_key(&puz, &k, 0.0);
        let qw = score_key(&puz, &k, 3.0);
        println!("{}: quad={:.1} word_bonus={:.2} qw={:.1}", name, q, (qw - q) / 3.0, qw);
    }
}
