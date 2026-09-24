//! Stage-by-stage debug of solve(): print key+scores after candidates,
//! rerank, pin_single_ai (reimplemented), bigram_polish.
//! Usage: stagedbg < puzzle.txt
use subst_solver::{build_locks, full_score, parse, replay, rerank_best, score_key, solve_candidates, Config, Solution};

fn pin_debug(puz: &subst_solver::Puzzle, sol: &Solution, cfg: &Config, locked_c: &[bool; 26]) -> Solution {
    // single-letter cipher words from raw text (avoids private Slot type)
    let mut counts = [0u32; 26];
    let mut cur: Vec<u8> = Vec::new();
    for &b in puz.raw.as_bytes().iter().chain(std::iter::once(&b' ')) {
        if b.is_ascii_alphabetic() {
            cur.push(b.to_ascii_uppercase() - b'A');
        } else {
            if cur.len() == 1 {
                counts[cur[0] as usize] += 1;
            }
            cur.clear();
        }
    }
    let mut order: Vec<u8> = (0..26u8).filter(|&c| counts[c as usize] > 0).collect();
    order.sort_by(|&a, &b| counts[b as usize].cmp(&counts[a as usize]));
    let mut best = sol.clone();
    let mut best_score = best.score;
    println!("  pin: initial best_score (combined scale) = {:.1}", best_score);
    for &c in order.iter().take(4) {
        if locked_c[c as usize] { continue; }
        for p in [b'A', b'I'] {
            let pv = p - b'A';
            if best.key[c as usize] == pv { continue; }
            let d = match (0..26u8).find(|&d| best.key[d as usize] == pv) {
                Some(d) => d, None => continue,
            };
            if locked_c[d as usize] { continue; }
            let mut key = best.key;
            key.swap(c as usize, d as usize);
            let s = score_key(puz, &key, cfg.word_w);
            println!("  pin: try cipher {} -> {} : score_key={:.1} vs best_score={:.1} {}",
                (b'A'+c) as char, p as char, s, best_score, if s > best_score { "APPLY" } else { "skip" });
            if s > best_score {
                best_score = s;
                best = Solution { key, score: s };
            }
        }
    }
    best
}

fn main() {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).unwrap();
    let text = text.trim_end_matches('\n');
    let puz = parse(text);
    let cfg = Config::default();
    let (locked_c, _) = build_locks(&[]);
    let winners = solve_candidates(&puz, &[], &cfg);
    println!("candidates: {}", winners.len());
    for (i, w) in winners.iter().take(3).enumerate() {
        let pl = replay(text, &w.key);
        println!("  cand{} qw={:.1} full={:.1} :: {}", i, score_key(&puz, &w.key, cfg.word_w), full_score(&puz, &w.key, &cfg), &pl[..pl.len().min(60)]);
    }
    // real rerank_best (combined = qw + bigram + trigram + name)
    let rb = rerank_best(&puz, &winners, &cfg);
    println!("rerank-best (real): qw={:.1} full={:.1} sol.score={:.1}", score_key(&puz, &rb.key, cfg.word_w), full_score(&puz, &rb.key, &cfg), rb.score);
    println!("  {}", &replay(text, &rb.key)[..80.min(text.len())]);
    let pinned = pin_debug(&puz, &rb, &cfg, &locked_c);
    println!("after pin: {}", &replay(text, &pinned.key)[..80.min(text.len())]);
}
