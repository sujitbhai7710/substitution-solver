//! Build data/models.bin: pruned, quantized language-model tables for the
//! Round-5 solver, trained on text8 (100MB Wikipedia plaintext).
//!
//! Layout (little-endian):
//!   u32 magic 0x324C444D ("MDL2"), u32 nsec,
//!   per section: u32 id, u32 byte_len, then section bytes.
//!
//! Sections:
//!   1: char 6-gram (26-symbol): u32 count, f32 floor, f32 scale, f32 ln_alpha;
//!      count x (u32 key, u8 q). key = 26-ary index, sorted ascending.
//!      logp = ln(c6/c5), stupid-backoff weight ln_alpha for missing.
//!   2: char 5-gram: same shape, logp = ln(c5/c4).
//!   3: char 4-gram full table: f32 floor, f32 scale, 26^4 u8. logp=ln(c4/c3).
//!   4: char 3-gram full table: f32 floor, f32 scale, 26^3 u8. logp=ln(c3/c2).
//!   5: char 2-gram full table: f32 floor, f32 scale, 26^2 u8. logp=ln(c2/c1).
//!   6: char unigram: 26 x f32 ln(c1/total).
//!   7: word bigram: u32 count, f32 floor, f32 scale, f32 oov;
//!      count x (u32 w1, u32 w2, u8 q), sorted by (w1,w2). logp = ln(c2/c1).
//!      Word ids are indices into words.bin vocabulary; oov = ln(1/V).
//!   8: word trigram: u32 count, f32 floor, f32 scale, f32 ln_alpha;
//!      count x (u64 key, u8 q), sorted ascending. key = (w1*V+w2)*V+w3.
//!      logp = ln(c3/c2).
//!
//! Pruning: top 2M 6-grams, top 1M 5-grams, top 500k word bigrams,
//! top 1M word trigrams by raw count.
//!
//! Usage: build_tables --text8 /tmp/text8data/text8 --words ../data/words.bin
//!                      --out ../data/models.bin

use std::collections::HashMap;
use std::io::Write;

const MAGIC: u32 = 0x324C_444D; // "MDL2"

fn get_arg(args: &[String], name: &str, dflt: &str) -> String {
    args.windows(2)
        .find(|w| w[0] == name)
        .map(|w| w[1].clone())
        .unwrap_or_else(|| dflt.to_string())
}

/// Read the words.bin vocabulary: word bytes -> id (index).
fn read_vocab(path: &str) -> (Vec<Vec<u8>>, HashMap<Vec<u8>, u32>) {
    let b = std::fs::read(path).expect("words.bin missing");
    let magic = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    assert_eq!(magic, 0x5752_4431, "words.bin bad magic");
    let count = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
    let mut words = Vec::with_capacity(count);
    let mut pos = 20usize;
    for _ in 0..count {
        let len = b[pos] as usize;
        pos += 1;
        words.push(b[pos..pos + len].to_vec());
        pos += len + 1; // skip qscore
    }
    let mut map = HashMap::with_capacity(count * 2);
    for (i, w) in words.iter().enumerate() {
        map.entry(w.clone()).or_insert(i as u32);
    }
    (words, map)
}

fn quantize(logps: &[f32]) -> (f32, f32, Vec<u8>) {
    let mut floor = f32::INFINITY;
    let mut ceil = f32::NEG_INFINITY;
    for &v in logps {
        if v < floor {
            floor = v;
        }
        if v > ceil {
            ceil = v;
        }
    }
    let scale = if ceil > floor {
        (ceil - floor) / 255.0
    } else {
        1.0
    };
    let qs: Vec<u8> = logps
        .iter()
        .map(|&v| ((v - floor) / scale).round().clamp(0.0, 255.0) as u8)
        .collect();
    (floor, scale, qs)
}

/// Keep top-n (key, count) by count using partial selection.
fn top_n(mut v: Vec<(u64, u32)>, n: usize) -> Vec<(u64, u32)> {
    if v.len() > n {
        v.select_nth_unstable_by(n, |a, b| b.1.cmp(&a.1));
        v.truncate(n);
    }
    v
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text8_path = get_arg(&args, "--text8", "/tmp/text8data/text8");
    let words_path = get_arg(&args, "--words", "../data/words.bin");
    let out_path = get_arg(&args, "--out", "../data/models.bin");

    println!("loading vocab…");
    let (_words, vocab) = read_vocab(&words_path);
    let v = _words.len() as u64;
    println!("vocab: {} words", v);

    println!("reading text8…");
    let raw = std::fs::read(&text8_path).expect("text8 missing");
    println!("text8 bytes: {}", raw.len());

    // ---- character n-gram counts (letters only, uppercase) ----
    println!("counting char n-grams…");
    let t = std::time::Instant::now();
    let mut c1 = [0u64; 26];
    let mut c2 = vec![0u64; 26 * 26];
    let mut c3 = vec![0u64; 26 * 26 * 26];
    let mut c4 = vec![0u64; 26 * 26 * 26 * 26];
    let mut c5: HashMap<u32, u32> = HashMap::new();
    let mut c6: HashMap<u32, u32> = HashMap::new();
    // rolling 26-ary indices
    let mut r2: u32 = 0;
    let mut r3: u32 = 0;
    let mut r4: u32 = 0;
    let mut r5: u32 = 0;
    let mut r6: u32 = 0;
    let mut n = 0u32; // letters in current run (no reset needed: text8 is clean)
    let mut total_letters = 0u64;
    for &b in &raw {
        let l = if b.is_ascii_lowercase() {
            b - b'a'
        } else if b.is_ascii_uppercase() {
            b - b'A'
        } else {
            continue;
        };
        let l = l as u32;
        c1[l as usize] += 1;
        total_letters += 1;
        r2 = (r2 * 26 + l) % (26 * 26);
        r3 = (r3 * 26 + l) % (26 * 26 * 26);
        r4 = r4.wrapping_mul(26).wrapping_add(l) % (26 * 26 * 26 * 26);
        // 26^5 = 11881376 fits u32; 26^6 = 308915776 fits u32
        r5 = (r5 * 26 + l) % 11_881_376;
        r6 = (r6 * 26 + l) % 308_915_776;
        n += 1;
        if n >= 2 {
            c2[r2 as usize] += 1;
        }
        if n >= 3 {
            c3[r3 as usize] += 1;
        }
        if n >= 4 {
            c4[r4 as usize] += 1;
        }
        if n >= 5 {
            *c5.entry(r5).or_insert(0) += 1;
        }
        if n >= 6 {
            *c6.entry(r6).or_insert(0) += 1;
        }
    }
    println!(
        "char counts done in {:.1}s: letters={} distinct5={} distinct6={}",
        t.elapsed().as_secs_f64(),
        total_letters,
        c5.len(),
        c6.len()
    );

    // ---- word n-gram counts ----
    println!("counting word n-grams…");
    let t = std::time::Instant::now();
    let mut w1 = vec![0u64; v as usize];
    let mut w2: HashMap<u64, u32> = HashMap::new();
    let mut w3: HashMap<u64, u32> = HashMap::new();
    let mut prev1: Option<u32> = None;
    let mut prev2: Option<u32> = None;
    let mut cur = Vec::<u8>::with_capacity(16);
    let mut tokens = 0u64;
    let mut flush = |cur: &mut Vec<u8>,
                     w1: &mut Vec<u64>,
                     w2: &mut HashMap<u64, u32>,
                     w3: &mut HashMap<u64, u32>,
                     prev1: &mut Option<u32>,
                     prev2: &mut Option<u32>,
                     tokens: &mut u64| {
        if cur.is_empty() {
            return;
        }
        *tokens += 1;
        let id = vocab.get(cur).copied();
        cur.clear();
        match id {
            Some(id) => {
                w1[id as usize] += 1;
                if let Some(p1) = *prev1 {
                    let k2 = p1 as u64 * v + id as u64;
                    *w2.entry(k2).or_insert(0) += 1;
                    if let Some(p2) = *prev2 {
                        let k3 = (p2 as u64 * v + p1 as u64) * v + id as u64;
                        *w3.entry(k3).or_insert(0) += 1;
                    }
                }
                *prev2 = *prev1;
                *prev1 = Some(id);
            }
            None => {
                *prev1 = None;
                *prev2 = None;
            }
        }
    };
    for &b in &raw {
        if b == b' ' || b == b'\n' {
            flush(&mut cur, &mut w1, &mut w2, &mut w3, &mut prev1, &mut prev2, &mut tokens);
        } else if b.is_ascii_lowercase() {
            cur.push(b - b'a' + b'A');
        } else if b.is_ascii_uppercase() {
            cur.push(b);
        } else {
            // unexpected byte: treat as separator
            flush(&mut cur, &mut w1, &mut w2, &mut w3, &mut prev1, &mut prev2, &mut tokens);
            prev1 = None;
            prev2 = None;
        }
    }
    flush(&mut cur, &mut w1, &mut w2, &mut w3, &mut prev1, &mut prev2, &mut tokens);
    println!(
        "word counts done in {:.1}s: tokens={} distinct2={} distinct3={}",
        t.elapsed().as_secs_f64(),
        tokens,
        w2.len(),
        w3.len()
    );

    // ---- build sections ----
    let mut sections: Vec<(u32, Vec<u8>)> = Vec::new();
    let ln = f64::ln;

    // Section 6: char unigram (26 f32)
    {
        let mut s = Vec::with_capacity(26 * 4);
        let lt = total_letters as f64;
        for i in 0..26 {
            s.extend_from_slice(&(ln(c1[i] as f64 / lt) as f32).to_le_bytes());
        }
        sections.push((6, s));
    }
    // Section 5: char bigram full (676 u8), logp = ln(c2/c1)
    {
        let mut lps = Vec::with_capacity(676);
        for a in 0..26 {
            let ca = c1[a] as f64;
            for b in 0..26 {
                let c = c2[a * 26 + b] as f64;
                lps.push(if c > 0.0 { ln(c / ca) as f32 } else { -20.0 });
            }
        }
        let (floor, scale, qs) = quantize(&lps);
        let mut s = Vec::with_capacity(8 + 676);
        s.extend_from_slice(&floor.to_le_bytes());
        s.extend_from_slice(&scale.to_le_bytes());
        s.extend_from_slice(&qs);
        sections.push((5, s));
        println!("sec5 bigram: floor={:.2} scale={:.4}", floor, scale);
    }
    // Section 4: char trigram full (17576 u8), logp = ln(c3/c2)
    {
        let mut lps = Vec::with_capacity(17576);
        for ab in 0..26 * 26 {
            let cab = c2[ab] as f64;
            for c in 0..26 {
                let cc = c3[ab * 26 + c] as f64;
                lps.push(if cc > 0.0 && cab > 0.0 {
                    ln(cc / cab) as f32
                } else {
                    -20.0
                });
            }
        }
        let (floor, scale, qs) = quantize(&lps);
        let mut s = Vec::with_capacity(8 + 17576);
        s.extend_from_slice(&floor.to_le_bytes());
        s.extend_from_slice(&scale.to_le_bytes());
        s.extend_from_slice(&qs);
        sections.push((4, s));
        println!("sec4 trigram: floor={:.2}", floor);
    }
    // Section 3: char 4-gram full (456976 u8), logp = ln(c4/c3)
    {
        println!("building 4-gram table…");
        let t = std::time::Instant::now();
        let mut lps = Vec::with_capacity(456976);
        for abc in 0..26 * 26 * 26 {
            let cabc = c3[abc] as f64;
            for d in 0..26 {
                let cc = c4[abc * 26 + d] as f64;
                lps.push(if cc > 0.0 && cabc > 0.0 {
                    ln(cc / cabc) as f32
                } else {
                    -20.0
                });
            }
        }
        let (floor, scale, qs) = quantize(&lps);
        let mut s = Vec::with_capacity(8 + 456976);
        s.extend_from_slice(&floor.to_le_bytes());
        s.extend_from_slice(&scale.to_le_bytes());
        s.extend_from_slice(&qs);
        sections.push((3, s));
        println!(
            "sec3 4-gram: floor={:.2} done in {:.1}s",
            floor,
            t.elapsed().as_secs_f64()
        );
    }
    // Section 2: char 5-gram top 1M, logp = ln(c5/c4)
    {
        println!("pruning 5-grams…");
        let t = std::time::Instant::now();
        let items: Vec<(u64, u32)> = c5.drain().map(|(k, c)| (k as u64, c)).collect();
        println!("  collected {} 5-grams", items.len());
        let top = top_n(items, 1_000_000);
        let mut kv: Vec<(u32, f32)> = Vec::with_capacity(top.len());
        for (k, c) in top {
            let ctx = (k / 26) as usize; // 4-gram context = key/26
            let cc = c4[ctx] as f64;
            let lp = if cc > 0.0 { ln(c as f64 / cc) as f32 } else { -20.0 };
            kv.push((k as u32, lp));
        }
        kv.sort_by_key(|&(k, _)| k);
        let lps: Vec<f32> = kv.iter().map(|&(_, l)| l).collect();
        let (floor, scale, qs) = quantize(&lps);
        let ln_alpha = (0.4f64).ln() as f32;
        let mut s = Vec::with_capacity(16 + kv.len() * 5);
        s.extend_from_slice(&(kv.len() as u32).to_le_bytes());
        s.extend_from_slice(&floor.to_le_bytes());
        s.extend_from_slice(&scale.to_le_bytes());
        s.extend_from_slice(&ln_alpha.to_le_bytes());
        for (i, &(k, _)) in kv.iter().enumerate() {
            s.extend_from_slice(&k.to_le_bytes());
            s.push(qs[i]);
        }
        sections.push((2, s));
        println!(
            "sec2 5-gram: kept {} floor={:.2} in {:.1}s",
            kv.len(),
            floor,
            t.elapsed().as_secs_f64()
        );
    }
    // Section 1: char 6-gram top 2M, logp = ln(c6/c5)
    {
        println!("pruning 6-grams…");
        let t = std::time::Instant::now();
        // Need c5 counts for contexts: recount c5 into a map for kept keys only.
        // We drained c5 above; rebuild context lookup from the kept 5-gram keys.
        // Simpler: recompute c5 for contexts on demand is expensive; instead
        // re-derive: we still have `top` 5-gram (k,c) pairs? They were moved.
        // Approach: before draining, we saved nothing. Redo: count pass is
        // deterministic — but easiest is to keep a HashMap of the KEPT 5-gram
        // counts. We lost them. Alternative: recompute c5 map quickly.
        //
        // Actually simplest: iterate raw again? No — instead, note c6 keys'
        // contexts: for each kept 6-gram key k6, context5 = k6 / 26.
        // We need c5(context5). Rebuild c5 map from raw (fast, ~10s).
        let mut c5b: HashMap<u32, u32> = HashMap::new();
        let mut r5: u32 = 0;
        let mut n = 0u32;
        for &b in &raw {
            let l = if b.is_ascii_lowercase() {
                b - b'a'
            } else if b.is_ascii_uppercase() {
                b - b'A'
            } else {
                continue;
            } as u32;
            r5 = (r5 * 26 + l) % 11_881_376;
            n += 1;
            if n >= 5 {
                *c5b.entry(r5).or_insert(0) += 1;
            }
        }
        let items: Vec<(u64, u32)> = c6.drain().map(|(k, c)| (k as u64, c)).collect();
        println!("  collected {} 6-grams", items.len());
        let top = top_n(items, 2_000_000);
        let mut kv: Vec<(u32, f32)> = Vec::with_capacity(top.len());
        for (k, c) in top {
            let ctx = (k / 26) as u32;
            let cc = c5b.get(&ctx).copied().unwrap_or(0) as f64;
            let lp = if cc > 0.0 { ln(c as f64 / cc) as f32 } else { -20.0 };
            kv.push((k as u32, lp));
        }
        kv.sort_by_key(|&(k, _)| k);
        let lps: Vec<f32> = kv.iter().map(|&(_, l)| l).collect();
        let (floor, scale, qs) = quantize(&lps);
        let ln_alpha = (0.4f64).ln() as f32;
        let mut s = Vec::with_capacity(16 + kv.len() * 5);
        s.extend_from_slice(&(kv.len() as u32).to_le_bytes());
        s.extend_from_slice(&floor.to_le_bytes());
        s.extend_from_slice(&scale.to_le_bytes());
        s.extend_from_slice(&ln_alpha.to_le_bytes());
        for (i, &(k, _)) in kv.iter().enumerate() {
            s.extend_from_slice(&k.to_le_bytes());
            s.push(qs[i]);
        }
        sections.push((1, s));
        println!(
            "sec1 6-gram: kept {} floor={:.2} in {:.1}s",
            kv.len(),
            floor,
            t.elapsed().as_secs_f64()
        );
    }
    // Section 7: word bigram top 500k, logp = ln(c2/c1w)
    {
        println!("pruning word bigrams…");
        let t = std::time::Instant::now();
        let items: Vec<(u64, u32)> = w2.drain().map(|(k, c)| (k, c)).collect();
        println!("  collected {} bigrams", items.len());
        let top = top_n(items, 500_000);
        let mut kv: Vec<(u32, u32, f32)> = Vec::with_capacity(top.len());
        for (k, c) in top {
            let w1id = (k / v) as u32;
            let w2id = (k % v) as u32;
            let c1w = w1[w1id as usize] as f64;
            let lp = if c1w > 0.0 { ln(c as f64 / c1w) as f32 } else { -20.0 };
            kv.push((w1id, w2id, lp));
        }
        kv.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
        let lps: Vec<f32> = kv.iter().map(|&(_, _, l)| l).collect();
        let (floor, scale, qs) = quantize(&lps);
        let oov = ln(1.0 / v as f64) as f32;
        let mut s = Vec::with_capacity(20 + kv.len() * 9);
        s.extend_from_slice(&(kv.len() as u32).to_le_bytes());
        s.extend_from_slice(&floor.to_le_bytes());
        s.extend_from_slice(&scale.to_le_bytes());
        s.extend_from_slice(&oov.to_le_bytes());
        for (i, &(a, b, _)) in kv.iter().enumerate() {
            s.extend_from_slice(&a.to_le_bytes());
            s.extend_from_slice(&b.to_le_bytes());
            s.push(qs[i]);
        }
        sections.push((7, s));
        println!(
            "sec7 word bigram: kept {} floor={:.2} oov={:.2} in {:.1}s",
            kv.len(),
            floor,
            oov,
            t.elapsed().as_secs_f64()
        );
    }
    // Section 8: word trigram top 1M, logp = ln(c3/c2)
    {
        println!("pruning word trigrams…");
        let t = std::time::Instant::now();
        // Need c2 for contexts: rebuild bigram map (we drained w2).
        let mut w2b: HashMap<u64, u32> = HashMap::new();
        {
            let mut prev1: Option<u32> = None;
            let mut prev2: Option<u32> = None;
            let mut cur = Vec::<u8>::with_capacity(16);
            for &b in &raw {
                if b == b' ' || b == b'\n' {
                    if !cur.is_empty() {
                        if let Some(id) = vocab.get(&cur) {
                            if let Some(p1) = prev1 {
                                let k2 = p1 as u64 * v + *id as u64;
                                *w2b.entry(k2).or_insert(0) += 1;
                            }
                            prev2 = prev1;
                            prev1 = Some(*id);
                        } else {
                            prev1 = None;
                            prev2 = None;
                        }
                        let _ = prev2;
                        cur.clear();
                    }
                } else if b.is_ascii_lowercase() {
                    cur.push(b - b'a' + b'A');
                } else if b.is_ascii_uppercase() {
                    cur.push(b);
                } else {
                    if !cur.is_empty() {
                        cur.clear();
                    }
                    prev1 = None;
                }
            }
        }
        let items: Vec<(u64, u32)> = w3.drain().map(|(k, c)| (k, c)).collect();
        println!("  collected {} trigrams", items.len());
        let top = top_n(items, 1_000_000);
        let mut kv: Vec<(u64, f32)> = Vec::with_capacity(top.len());
        for (k, c) in top {
            let ctx = k / v; // (w1*V+w2)
            let cc = w2b.get(&ctx).copied().unwrap_or(0) as f64;
            let lp = if cc > 0.0 { ln(c as f64 / cc) as f32 } else { -20.0 };
            kv.push((k, lp));
        }
        kv.sort_by_key(|&(k, _)| k);
        let lps: Vec<f32> = kv.iter().map(|&(_, l)| l).collect();
        let (floor, scale, qs) = quantize(&lps);
        let ln_alpha = (0.4f64).ln() as f32;
        let mut s = Vec::with_capacity(24 + kv.len() * 9);
        s.extend_from_slice(&(kv.len() as u32).to_le_bytes());
        s.extend_from_slice(&floor.to_le_bytes());
        s.extend_from_slice(&scale.to_le_bytes());
        s.extend_from_slice(&ln_alpha.to_le_bytes());
        s.extend_from_slice(&v.to_le_bytes()); // vocab size V for key decoding
        for (i, &(k, _)) in kv.iter().enumerate() {
            s.extend_from_slice(&k.to_le_bytes());
            s.push(qs[i]);
        }
        sections.push((8, s));
        println!(
            "sec8 word trigram: kept {} floor={:.2} in {:.1}s",
            kv.len(),
            floor,
            t.elapsed().as_secs_f64()
        );
    }

    // ---- write models.bin ----
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&(sections.len() as u32).to_le_bytes());
    // Sort sections by id for determinism.
    sections.sort_by_key(|&(id, _)| id);
    for (id, s) in &sections {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&(s.len() as u32).to_le_bytes());
        out.extend_from_slice(s);
    }
    let mut f = std::fs::File::create(&out_path).expect("create models.bin");
    f.write_all(&out).unwrap();
    println!("wrote {}: {} bytes", out_path, out.len());
}
