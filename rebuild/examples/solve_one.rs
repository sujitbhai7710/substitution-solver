use subst_solver::{parse, parse_clue, replay, score_key, solve, Config};

fn main() {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).unwrap();
    let text = text.trim_end_matches('\n');
    let puz = parse(text);
    let mut cfg = Config::default();
    // Every knob defaults to Config::default() (the production/WASM config);
    // env vars only override for tuning. (Beam stays off unless BEAM=1.)
    cfg.restarts = std::env::var("R").ok().and_then(|v| v.parse().ok()).unwrap_or(cfg.restarts);
    cfg.steps = std::env::var("S").ok().and_then(|v| v.parse().ok()).unwrap_or(cfg.steps);
    cfg.t0 = std::env::var("T0").ok().and_then(|v| v.parse().ok()).unwrap_or(cfg.t0);
    cfg.t_end = std::env::var("TE").ok().and_then(|v| v.parse().ok()).unwrap_or(cfg.t_end);
    cfg.word_w = std::env::var("WW").ok().and_then(|v| v.parse().ok()).unwrap_or(cfg.word_w);
    cfg.beam_width = std::env::var("BW").ok().and_then(|v| v.parse().ok()).unwrap_or(cfg.beam_width);
    cfg.use_beam = std::env::var("BEAM").is_ok();
    cfg.bigram_w = std::env::var("BG").ok().and_then(|v| v.parse().ok()).unwrap_or(cfg.bigram_w);
    cfg.trigram_w = std::env::var("TW").ok().and_then(|v| v.parse().ok()).unwrap_or(cfg.trigram_w);
    cfg.name_w = std::env::var("NW").ok().and_then(|v| v.parse().ok()).unwrap_or(cfg.name_w);
    cfg.use_pattern_seed = std::env::var("NOPAT").is_err();
    cfg.seed = std::env::var("SEED").ok().and_then(|v| u64::from_str_radix(v.trim_start_matches("0x"), 16).ok()).unwrap_or(0x1234_5678_9abc_def0);
    let t = std::time::Instant::now();
    let sol = solve(&puz, &parse_clue(text), &cfg);
    eprintln!("ms={:.0} score={:.1}", t.elapsed().as_secs_f64() * 1000.0, sol.score);
    println!("{}", replay(text, &sol.key));
    // TRUE=<answer> scores the true key under the same model (model vs search diagnosis).
    if let Ok(ans) = std::env::var("TRUE") {
        let pc: Vec<u8> = text.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
        let ac: Vec<u8> = ans.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
        let mut key = [0u8; 26];
        let mut seen = [false; 26];
        for (&pb, &ab) in pc.iter().zip(ac.iter()) {
            let c = (pb.to_ascii_uppercase() - b'A') as usize;
            if !seen[c] {
                seen[c] = true;
                key[c] = ab.to_ascii_uppercase() - b'A';
            }
        }
        let mut full = true;
        for c in 0..26 {
            if puz.letters.contains(&(c as u8)) && !seen[c] {
                full = false;
            }
        }
        eprintln!(
            "true-key score={:.1} (full={})",
            score_key(&puz, &key, cfg.word_w),
            full
        );
    }
}
