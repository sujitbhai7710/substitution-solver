//! Native benchmark: solve every internally-consistent corpus entry and
//! report exact-match accuracy per puzzle type plus timing stats.
//!
//! Usage: bench --corpus <corpus.json> [--restarts N] [--steps N]
//!              [--threads T] [--limit N] [--out results.jsonl]
//!              [--word-w W] [--t0 T] [--t-end T] [--no-pat]
//! Build: cargo build --release --features parallel --bin bench

use rayon::prelude::*;
use std::time::Instant;
use subst_solver::{exact_match, parse, replay, solve, substitution_consistent, Config};

// ---------- minimal JSON parser (enough for the corpus format) ----------

struct P<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn peek(&mut self) -> u8 {
        self.ws();
        self.b[self.i]
    }
    fn expect(&mut self, c: u8) {
        assert_eq!(self.peek(), c, "json parse error at {}", self.i);
        self.i += 1;
    }
    fn hex4(&mut self) -> u32 {
        let mut v = 0u32;
        for _ in 0..4 {
            v = v * 16
                + match self.b[self.i] {
                    b'0'..=b'9' => (self.b[self.i] - b'0') as u32,
                    b'a'..=b'f' => (self.b[self.i] - b'a' + 10) as u32,
                    b'A'..=b'F' => (self.b[self.i] - b'A' + 10) as u32,
                    _ => panic!("bad hex"),
                };
            self.i += 1;
        }
        v
    }
    fn string(&mut self) -> String {
        self.expect(b'"');
        let mut out = String::new();
        loop {
            let c = self.b[self.i];
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => match self.b[self.i] {
                    b'"' => {
                        out.push('"');
                        self.i += 1;
                    }
                    b'\\' => {
                        out.push('\\');
                        self.i += 1;
                    }
                    b'/' => {
                        out.push('/');
                        self.i += 1;
                    }
                    b'n' => {
                        out.push('\n');
                        self.i += 1;
                    }
                    b'r' => {
                        out.push('\r');
                        self.i += 1;
                    }
                    b't' => {
                        out.push('\t');
                        self.i += 1;
                    }
                    b'u' => {
                        self.i += 1;
                        let mut cp = self.hex4();
                        // surrogate pair
                        if (0xD800..0xDC00).contains(&cp) && self.b[self.i] == b'\\' {
                            let save = self.i;
                            self.i += 1;
                            if self.b[self.i] == b'u' {
                                self.i += 1;
                                let lo = self.hex4();
                                if (0xDC00..0xE000).contains(&lo) {
                                    cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                } else {
                                    self.i = save;
                                }
                            } else {
                                self.i = save;
                            }
                        }
                        out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                    }
                    _ => panic!("bad escape"),
                },
                _ => {
                    // raw UTF-8 byte(s)
                    let start = self.i - 1;
                    let len = utf8_len(c);
                    let s = std::str::from_utf8(&self.b[start..start + len]).unwrap();
                    out.push_str(s);
                    self.i = start + len;
                }
            }
        }
        out
    }
}

fn utf8_len(c: u8) -> usize {
    if c < 0x80 {
        1
    } else if c >> 5 == 0b110 {
        2
    } else if c >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

#[derive(Clone)]
struct Entry {
    typ: String,
    date: String,
    puzzle: String,
    answer: String,
}

fn parse_corpus(text: &str) -> Vec<Entry> {
    let mut p = P {
        b: text.as_bytes(),
        i: 0,
    };
    p.expect(b'[');
    let mut out = Vec::new();
    loop {
        if p.peek() == b']' {
            p.i += 1;
            break;
        }
        p.expect(b'{');
        let (mut typ, mut date, mut puzzle, mut answer) =
            (String::new(), String::new(), String::new(), String::new());
        loop {
            let k = p.string();
            p.expect(b':');
            let v = p.string();
            match k.as_str() {
                "type" => typ = v,
                "date" => date = v,
                "puzzle" => puzzle = v,
                "answer" => answer = v,
                _ => {}
            }
            if p.peek() == b',' {
                p.i += 1;
            } else {
                break;
            }
        }
        p.expect(b'}');
        out.push(Entry {
            typ,
            date,
            puzzle,
            answer,
        });
        if p.peek() == b',' {
            p.i += 1;
        }
    }
    out
}

// ---------- benchmark ----------

#[derive(Clone)]
struct Row {
    typ: String,
    date: String,
    ms: f64,
    ok: bool,
    winnable: bool,
    got: String,
    puzzle: String,
    answer: String,
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn report(rows: &[Row], label: &str) {
    let n = rows.len();
    let ok = rows.iter().filter(|r| r.ok).count();
    let times: Vec<f64> = rows.iter().map(|r| r.ms).collect();
    let mean = if n > 0 {
        times.iter().sum::<f64>() / n as f64
    } else {
        0.0
    };
    println!(
        "{:<16} n={:<5} acc={:>7.2}%  ok={:<5} median={:>8.1}ms mean={:>8.1}ms",
        label,
        n,
        if n > 0 { 100.0 * ok as f64 / n as f64 } else { 0.0 },
        ok,
        median(times),
        mean
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut get = |name: &str, dflt: &str| -> String {
        args.windows(2)
            .find(|w| w[0] == name)
            .map(|w| w[1].clone())
            .unwrap_or_else(|| dflt.to_string())
    };
    let corpus_path = get("--corpus", "");
    assert!(!corpus_path.is_empty(), "--corpus required");
    let restarts: usize = get("--restarts", "24").parse().unwrap();
    let steps: usize = get("--steps", "12000").parse().unwrap();
    let threads: usize = get("--threads", "0").parse().unwrap();
    let limit: usize = get("--limit", "0").parse().unwrap();
    let word_w: f64 = get("--word-w", "1.5").parse().unwrap();
    let bigram_w: f64 = get("--bigram-w", "1.0").parse().unwrap();
    let trigram_w: f64 = get("--trigram-w", "1.0").parse().unwrap();
    let t0: f64 = get("--t0", "20.0").parse().unwrap();
    let t_end: f64 = get("--t-end", "0.02").parse().unwrap();
    let use_pat = !args.contains(&"--no-pat".to_string());
    let use_beam = !args.contains(&"--no-beam".to_string());
    let chi: f64 = get("--chi", "0.5").parse().unwrap();
    let beam_width: usize = get("--beam-width", "5000").parse().unwrap();
    let beam_nbest: usize = get("--beam-nbest", "12").parse().unwrap();
    let dict_nodes: u64 = get("--dict-nodes", "200000").parse().unwrap();
    let cand_cap: usize = get("--cand-cap", "2000").parse().unwrap();
    let out_path = get("--out", "");
    let miss_path = get("--dump-misses", "");

    if threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .ok();
    }

    println!("loading corpus…");
    let text = std::fs::read_to_string(&corpus_path).unwrap();
    let entries = parse_corpus(&text);
    println!("total entries: {}", entries.len());

    // Filter to internally-consistent ciphers (full consistent set).
    // Winnable entries (puzzle/answer punctuation byte-identical) are the
    // set on which exact match is achievable; the rest are corpus
    // transcription artifacts (e.g. en-dash vs minus sign) that no
    // verbatim-replay solver can match by construction.
    let entries: Vec<(Entry, bool)> = entries
        .into_iter()
        .filter(|e| substitution_consistent(&e.puzzle, &e.answer))
        .map(|e| {
            let w = {
                let sp: Vec<u8> = e
                    .puzzle
                    .bytes()
                    .filter(|b| !b.is_ascii_alphabetic())
                    .collect();
                let sa: Vec<u8> = e
                    .answer
                    .bytes()
                    .filter(|b| !b.is_ascii_alphabetic())
                    .collect();
                sp == sa
            };
            (e, w)
        })
        .collect();
    println!("consistent entries: {}", entries.len());
    println!(
        "exact-winnable entries: {}",
        entries.iter().filter(|(_, w)| *w).count()
    );
    let entries: Vec<(Entry, bool)> = if limit > 0 {
        entries.into_iter().take(limit).collect()
    } else {
        entries
    };

    // Warm models once before timing.
    subst_solver::warmup();

    let cfg = Config {
        restarts,
        steps,
        t0,
        t_end,
        word_w,
        bigram_w,
        trigram_w,
        dict_nodes,
        dict_top_k: 6,
        dict_cand_cap: cand_cap,
        use_pattern_seed: use_pat,
        seed: 0x9E37_79B9_7F4A_7C15,
        use_correction: false,
        chi,
        use_beam,
        beam_width,
        beam_nbest,
        pin_single_ai: true,
        name_w: 10.0,
    };

    let t_all = Instant::now();
    let rows: Vec<Row> = entries
        .par_iter()
        .map(|(e, winnable)| {
            let t = Instant::now();
            let puz = parse(&e.puzzle);
            let sol = solve(&puz, &[], &cfg);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            let plain = replay(&puz.raw, &sol.key);
            let ok = exact_match(&plain, &e.answer);
            Row {
                typ: e.typ.clone(),
                date: e.date.clone(),
                ms,
                ok,
                winnable: *winnable,
                got: if ok { String::new() } else { plain },
                puzzle: if ok { String::new() } else { e.puzzle.clone() },
                answer: if ok { String::new() } else { e.answer.clone() },
            }
        })
        .collect();
    let wall = t_all.elapsed().as_secs_f64();
    println!("wall time: {:.1}s for {} entries", wall, rows.len());

    println!("\n== FULL CONSISTENT SET ==");
    for typ in ["cryptoquip", "cryptoquote", "celebrity-cipher"] {
        let sub: Vec<Row> = rows.iter().filter(|r| r.typ == typ).cloned().collect();
        report(&sub, typ);
    }
    report(&rows, "OVERALL");

    println!("\n== EXACT-WINNABLE SUBSET (exact match achievable) ==");
    let win: Vec<Row> = rows.iter().filter(|r| r.winnable).cloned().collect();
    for typ in ["cryptoquip", "cryptoquote", "celebrity-cipher"] {
        let sub: Vec<Row> = win.iter().filter(|r| r.typ == typ).cloned().collect();
        report(&sub, typ);
    }
    report(&win, "OVERALL");

    // Misses list (first 40) for diagnosis.
    let misses: Vec<&Row> = rows.iter().filter(|r| !r.ok).take(40).collect();
    if !misses.is_empty() {
        println!("\nsample misses ({} total):", rows.iter().filter(|r| !r.ok).count());
        for m in misses {
            println!("  {} {}", m.typ, m.date);
        }
    }

    if !out_path.is_empty() {
        let mut f = String::new();
        for r in &rows {
            f.push_str(&format!(
                "{{\"type\":\"{}\",\"date\":\"{}\",\"ms\":{:.1},\"ok\":{}}}\n",
                r.typ, r.date, r.ms, r.ok
            ));
        }
        std::fs::write(&out_path, f).unwrap();
        println!("wrote {}", out_path);
    }

    if !miss_path.is_empty() {
        let mut f = String::new();
        let esc = |v: &str| {
            v.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
        };
        for r in rows.iter().filter(|r| !r.ok) {
            f.push_str(&format!(
                "{{\"type\":\"{}\",\"date\":\"{}\",\"puzzle\":\"{}\",\"answer\":\"{}\",\"got\":\"{}\"}}\n",
                r.typ, r.date, esc(&r.puzzle), esc(&r.answer), esc(&r.got)
            ));
        }
        std::fs::write(&miss_path, f).unwrap();
        println!("wrote {} misses to {}", rows.iter().filter(|r| !r.ok).count(), miss_path);
    }
}


