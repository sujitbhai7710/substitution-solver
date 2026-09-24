//! Debug: for a given puzzle+answer, solve, derive true key, and check whether
//! any SINGLE swap of the solver's final key improves score_key (quad+word).
//! Usage: swapdbg < puzzle.txt ; TRUE=<answer> in env.
use subst_solver::{parse, replay, score_key, solve, Config};

fn main() {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).unwrap();
    let text = text.trim_end_matches('\n');
    let puz = parse(text);
    let cfg = Config::default();
    let sol = solve(&puz, &[], &cfg);
    let got = replay(text, &sol.key);
    println!("got: {}", got);
    println!("score_key(got): {:.1}", score_key(&puz, &sol.key, cfg.word_w));

    let ans = std::env::var("TRUE").expect("TRUE env required");
    let pc: Vec<u8> = text.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
    let ac: Vec<u8> = ans.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
    assert_eq!(pc.len(), ac.len(), "letter count mismatch");
    let mut tk = [0u8; 26];
    let mut seen = [false; 26];
    let mut seenp = [false; 26];
    for (&pb, &ab) in pc.iter().zip(ac.iter()) {
        let c = (pb.to_ascii_uppercase() - b'A') as usize;
        let p = (ab.to_ascii_uppercase() - b'A') as usize;
        if seen[c] {
            assert_eq!(tk[c] as usize, p, "inconsistent label");
        } else {
            assert!(!seenp[p], "inconsistent label");
            seen[c] = true;
            seenp[p] = true;
            tk[c] = p as u8;
        }
    }
    let mut used = [false; 26];
    for c in 0..26 {
        if seen[c] {
            used[tk[c] as usize] = true;
        }
    }
    let free: Vec<u8> = (0..26u8).filter(|p| !used[*p as usize]).collect();
    let mut fi = 0;
    for c in 0..26 {
        if !seen[c] {
            tk[c] = free[fi];
            fi += 1;
        }
    }
    println!("score_key(truth): {:.1}", score_key(&puz, &tk, cfg.word_w));

    // All single swaps of the solver's key: which is best under score_key?
    let base = score_key(&puz, &sol.key, cfg.word_w);
    let mut best = (0.0f64, 0u8, 0u8);
    for i in 0..26u8 {
        for j in i + 1..26u8 {
            let mut k = sol.key;
            k.swap(i as usize, j as usize);
            let s = score_key(&puz, &k, cfg.word_w);
            if s - base > best.0 {
                best = (s - base, i, j);
            }
        }
    }
    println!(
        "best single swap of got key: +{:.1} (cipher {} <-> {})",
        best.0,
        (b'A' + best.1) as char,
        (b'A' + best.2) as char
    );
    // Is got one swap from truth?
    let mut diffs = Vec::new();
    for c in 0..26 {
        if sol.key[c] != tk[c] {
            diffs.push(c);
        }
    }
    println!("key positions differing from truth: {:?}", diffs.iter().map(|&c| (b'A'+c as u8) as char).collect::<Vec<_>>());
}
