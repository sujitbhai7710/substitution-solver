//! Debug: print solve_candidates N-best for stdin puzzle.
use subst_solver::{parse, parse_clue, replay, solve_candidates, Config};

fn main() {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).unwrap();
    let text = text.trim_end_matches('\n');
    let puz = parse(text);
    let mut cfg = Config::default();
    if std::env::var("NOBEAM").is_ok() {
        cfg.use_beam = false;
    }
    let winners = solve_candidates(&puz, &parse_clue(text), &cfg);
    eprintln!("n={}", winners.len());
    for (i, w) in winners.iter().enumerate() {
        let pt = replay(text, &w.key);
        let first: String = pt.split_whitespace().take(4).collect::<Vec<_>>().join(" ");
        eprintln!("{i}: score={:.1} {first}", w.score);
    }
}
