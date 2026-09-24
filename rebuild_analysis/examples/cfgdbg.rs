//! Test search configs on specific puzzles. Usage: cfgdbg < puzzles.txt (id<TAB>puzzle<TAB>answer per line)
//! Env: R, S, T0, TEND
use subst_solver::{parse, replay, solve, Config};

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphabetic()).map(|c| c.to_ascii_uppercase()).collect()
}

fn main() {
    let mut input = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut input).unwrap();
    let r: usize = std::env::var("R").ok().and_then(|v| v.parse().ok()).unwrap_or(48);
    let s: usize = std::env::var("S").ok().and_then(|v| v.parse().ok()).unwrap_or(12000);
    let t0: f64 = std::env::var("T0").ok().and_then(|v| v.parse().ok()).unwrap_or(10.0);
    let tend: f64 = std::env::var("TEND").ok().and_then(|v| v.parse().ok()).unwrap_or(0.02);
    let mut ok = 0; let mut n = 0;
    for line in input.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 { continue; }
        let (id, puz_t, ans_t) = (parts[0], parts[1], parts[2]);
        let puz = parse(puz_t);
        let cfg = Config { restarts: r, steps: s, t0, t_end: tend, ..Config::default() };
        let t = std::time::Instant::now();
        let sol = solve(&puz, &[], &cfg);
        let out = replay(puz_t, &sol.key);
        let good = norm(&out) == norm(ans_t);
        if good { ok += 1; } n += 1;
        println!("{} {} {:.1}s {}", id, if good {"OK"} else {"MISS"}, t.elapsed().as_secs_f64(), &out[..out.len().min(60)]);
    }
    println!("RESULT: {}/{} with R={} S={} t0={}", ok, n, r, s, t0);
}
