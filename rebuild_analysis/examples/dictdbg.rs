//! Debug dict_attack on a puzzle from stdin.
use subst_solver::{dict_attack, parse, replay};

fn main() {
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).unwrap();
    let text = text.trim_end_matches('\n');
    let puz = parse(text);
    let t = std::time::Instant::now();
    let hits = dict_attack(&puz, &[false; 26], &[0; 26], 200_000, 3, 2000);
    println!("hits: {} in {:.1}s", hits.len(), t.elapsed().as_secs_f64());
    for (i, h) in hits.iter().enumerate() {
        let pt = replay(text, &{
            let mut k = [0u8; 26];
            for c in 0..26 {
                k[c] = if h.key[c] == 255 { 0 } else { h.key[c] };
            }
            k
        });
        println!("hit{i}: score={:.1} mapped={} :: {}", h.score, h.mapped_letters, &pt[..pt.len().min(70)]);
    }
}
