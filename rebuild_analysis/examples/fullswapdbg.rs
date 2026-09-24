//! Check if solver output is a single-swap local optimum under full_score.
//! Usage: fullswapdbg < puzzle.txt
use subst_solver::{full_score, parse, replay, solve, Config};

fn main() {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).unwrap();
    let text = text.trim_end_matches('\n');
    let puz = parse(text);
    let cfg = Config::default();
    let sol = solve(&puz, &[], &cfg);
    let base = full_score(&puz, &sol.key, &cfg);
    println!("got: {}", &replay(text, &sol.key)[..text.len().min(80)]);
    println!("full_score(got)={:.1}", base);
    let mut best_d = 0.0;
    let mut best_sw = (0, 0);
    for a in 0..26 {
        for b in (a + 1)..26 {
            let mut k = sol.key;
            // swap plain images of cipher a,b
            let pa = k[a]; let pb = k[b];
            k[a] = pb; k[b] = pa;
            // check bijective (swap preserves it)
            let s = full_score(&puz, &k, &cfg);
            if s - base > best_d {
                best_d = s - base;
                best_sw = (a, b);
            }
        }
    }
    println!("best single swap under full_score: +{:.1} ({}<->{})", best_d, (best_sw.0 + 65) as u8 as char, (best_sw.1 + 65) as u8 as char);
}
