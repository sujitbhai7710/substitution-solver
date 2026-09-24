//! Test: how many restarts to find truth? Reports best qw vs truth qw.
//! Usage: restartdbg < puzzle.txt ; TRUE=<answer> in env. R=<restarts> S=<steps>
use subst_solver::{parse, score_key, solve_candidates, Config};

fn main() {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).unwrap();
    let text = text.trim_end_matches('\n');
    let puz = parse(text);
    let ans = std::env::var("TRUE").expect("TRUE env required");
    let pc: Vec<u8> = text.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
    let ac: Vec<u8> = ans.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
    let mut tk = [0u8; 26];
    let mut seen = [false; 26];
    let mut seenp = [false; 26];
    for (&pb, &ab) in pc.iter().zip(ac.iter()) {
        let c = (pb.to_ascii_uppercase() - b'A') as usize;
        let p = (ab.to_ascii_uppercase() - b'A') as usize;
        if !seen[c] {
            seen[c] = true; seenp[p] = true; tk[c] = p as u8;
        }
    }
    let mut used = [false; 26];
    for c in 0..26 { if seen[c] { used[tk[c] as usize] = true; } }
    let free: Vec<u8> = (0..26u8).filter(|p| !used[*p as usize]).collect();
    let mut fi = 0;
    for c in 0..26 { if !seen[c] { tk[c] = free[fi]; fi += 1; } }
    let tq = score_key(&puz, &tk, 3.0);
    println!("truth qw={:.1}", tq);

    let r: usize = std::env::var("R").ok().and_then(|v| v.parse().ok()).unwrap_or(48);
    let s: usize = std::env::var("S").ok().and_then(|v| v.parse().ok()).unwrap_or(12000);
    let mut cfg = Config::default();
    cfg.restarts = r;
    cfg.steps = s;
    let t = std::time::Instant::now();
    let winners = solve_candidates(&puz, &[], &cfg);
    println!("restarts={} steps={} time={:.1}s", r, s, t.elapsed().as_secs_f64());
    let mut best = f64::NEG_INFINITY;
    let mut nbetter = 0;
    for w in &winners {
        if w.score > best { best = w.score; }
        if w.score > tq { nbetter += 1; }
    }
    println!("best candidate qw={:.1} (truth {:.1}), candidates beating truth: {}", best, tq, nbetter);
}
