use subst_solver::{parse, replay, solve_candidates, Config};

fn main() {
    let text = "OPNV CPOO YVCWYU GMA, DAB IMB WOCWGR DG BXV YMABV GMA VSEVFB. – VUIW YMUYPJAVL";
    let puz = parse(text);
    let cfg = Config {
        restarts: 8,
        steps: 6000,
        ..Config::default()
    };
    let winners = solve_candidates(&puz, &[], &cfg);
    println!("distinct winners: {}", winners.len());
    for (i, w) in winners.iter().take(8).enumerate() {
        let pt = replay(&puz.raw, &w.key);
        let first_word: String = pt.split_whitespace().next().unwrap_or("").to_string();
        println!("{i}: score={:.1} first_word={first_word}", w.score);
    }
}
