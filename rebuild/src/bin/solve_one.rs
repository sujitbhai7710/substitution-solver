//! solve_one: single-puzzle CLI for the daily pipeline.
//! Usage: solve_one --puzzle "CIPHERTEXT" --clue "X=Y" [--restarts 192] [--steps 6000]
//! Prints one JSON line: {"plaintext":..., "key":..., "score":..., "ms":...}
use subst_solver::{solve_text, warmup, Config};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str, dflt: &str| -> String {
        args.windows(2)
            .find(|w| w[0] == name)
            .map(|w| w[1].clone())
            .unwrap_or_else(|| dflt.to_string())
    };
    let puzzle = get("--puzzle", "");
    if puzzle.is_empty() {
        eprintln!("solve_one: --puzzle required");
        std::process::exit(2);
    }
    let clue = get("--clue", "");
    let restarts: usize = get("--restarts", "192").parse().unwrap_or(192);
    let steps: usize = get("--steps", "6000").parse().unwrap_or(6000);

    warmup();
    let mut cfg = Config::default();
    cfg.restarts = restarts;
    cfg.steps = steps;
    let r = solve_text(&puzzle, &clue, &cfg);

    let esc = |s: &str| -> String {
        let mut o = String::with_capacity(s.len() + 2);
        for ch in s.chars() {
            match ch {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                '\n' => o.push_str("\\n"),
                '\r' => o.push_str("\\r"),
                c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                c => o.push(c),
            }
        }
        o
    };
    println!(
        "{{\"plaintext\":\"{}\",\"key\":\"{}\",\"score\":{:.3},\"ms\":{:.1}}}",
        esc(&r.plaintext),
        esc(&r.key),
        r.score,
        r.ms
    );
}
