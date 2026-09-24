//! Batch triage: solve every corpus entry at production config and classify
//! each failure as label-noise / search-error / model-error.
//!
//! Scoring uses the letter-normalized exact match (lowercase, a-z only,
//! full-string equality).
//!
//! Usage: triage --corpus <corpus.json> [--restarts 48] [--steps 12000]
//!               [--threads 2] [--limit N] [--out rows.jsonl]
//! Build: cargo build --release --features parallel --bin triage

use rayon::prelude::*;
use std::time::Instant;
use subst_solver::{
    exact_match, exact_winnable, full_score, parse, replay, score_key, solve,
    substitution_consistent, Config, Puzzle,
};

// ---------- minimal JSON parser (same as bench.rs) ----------

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

// ---------- analysis ----------

/// Letters-only lowercase normalization (canonical scorer).
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Rebuild the true key from a consistent label. Returns None when the label
/// is not substitution-consistent with the puzzle.
fn true_key(puz: &Puzzle, puzzle: &str, ans: &str) -> Option<[u8; 26]> {
    let pc: Vec<u8> = puzzle.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
    let ac: Vec<u8> = ans.bytes().filter(|b| b.is_ascii_alphabetic()).collect();
    if pc.len() != ac.len() {
        return None;
    }
    let mut key = [0u8; 26];
    let mut seen = [false; 26];
    let mut seen_p = [false; 26];
    for (&pb, &ab) in pc.iter().zip(ac.iter()) {
        let c = (pb.to_ascii_uppercase() - b'A') as usize;
        let pl = (ab.to_ascii_uppercase() - b'A') as usize;
        if seen[c] {
            if key[c] as usize != pl {
                return None;
            }
        } else if seen_p[pl] {
            return None;
        } else {
            seen[c] = true;
            seen_p[pl] = true;
            key[c] = pl as u8;
        }
    }
    let mut used = [false; 26];
    for c in 0..26 {
        if seen[c] {
            used[key[c] as usize] = true;
        }
    }
    let free: Vec<u8> = (0..26u8).filter(|p| !used[*p as usize]).collect();
    let mut fi = 0;
    for c in 0..26 {
        if !seen[c] {
            key[c] = free[fi];
            fi += 1;
        }
    }
    Some(key)
}

#[derive(Clone)]
struct Row {
    typ: String,
    date: String,
    ms: f64,
    consistent: bool,
    winnable: bool,
    ok_byte: bool,
    ok_norm: bool,
    verdict: String,
    qw_truth: f64,
    qw_got: f64,
    full_truth: f64,
    full_got: f64,
    nletters: usize,
    nwords: usize,
    got: String,
    answer: String,
    puzzle: String,
}

fn esc(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
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
    let restarts: usize = get("--restarts", "48").parse().unwrap();
    let steps: usize = get("--steps", "12000").parse().unwrap();
    let threads: usize = get("--threads", "2").parse().unwrap();
    let limit: usize = get("--limit", "0").parse().unwrap();
    let t0: f64 = get("--t0", "10.0").parse().unwrap();
    let t_end: f64 = get("--t-end", "0.02").parse().unwrap();
    let word_w: f64 = get("--word-w", "3.0").parse().unwrap();
    let bigram_w: f64 = get("--bigram-w", "2.0").parse().unwrap();
    let trigram_w: f64 = get("--trigram-w", "1.0").parse().unwrap();
    let name_w: f64 = get("--name-w", "10.0").parse().unwrap();
    let out_path = get("--out", "");
    assert!(!out_path.is_empty(), "--out required");

    if threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .ok();
    }

    let text = std::fs::read_to_string(&corpus_path).unwrap();
    let mut entries = parse_corpus(&text);
    if limit > 0 {
        entries.truncate(limit);
    }
    println!("entries: {}", entries.len());

    subst_solver::warmup();

    let cfg = Config {
        restarts,
        steps,
        t0,
        t_end,
        word_w,
        bigram_w,
        trigram_w,
        name_w,
        pin_single_ai: true,
        use_pattern_seed: true,
        seed: 0x9E37_79B9_7F4A_7C15,
        use_correction: false,
        ..Config::default()
    };

    let t_all = Instant::now();
    let rows: Vec<Row> = entries
        .par_iter()
        .map(|e| {
            let t = Instant::now();
            let puz = parse(&e.puzzle);
            let consistent = substitution_consistent(&e.puzzle, &e.answer);
            let winnable = exact_winnable(&e.puzzle, &e.answer);
            let sol = solve(&puz, &[], &cfg);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            let plain = replay(&puz.raw, &sol.key);
            let ok_byte = exact_match(&plain, &e.answer);
            let ok_norm = norm(&plain) == norm(&e.answer);
            let (verdict, qw_truth, qw_got, full_truth, full_got) = if ok_norm {
                ("ok".to_string(), 0.0, 0.0, 0.0, 0.0)
            } else if !consistent {
                ("label-noise".to_string(), 0.0, 0.0, 0.0, 0.0)
            } else {
                match true_key(&puz, &e.puzzle, &e.answer) {
                    Some(tk) => {
                        let qt = score_key(&puz, &tk, cfg.word_w);
                        let qg = score_key(&puz, &sol.key, cfg.word_w);
                        let ft = full_score(&puz, &tk, &cfg);
                        let fg = full_score(&puz, &sol.key, &cfg);
                        let v = if ft > fg + 1e-6 {
                            "search-error"
                        } else if fg > ft + 1e-6 {
                            "model-error"
                        } else {
                            "tie"
                        };
                        (v.to_string(), qt, qg, ft, fg)
                    }
                    None => ("label-noise".to_string(), 0.0, 0.0, 0.0, 0.0),
                }
            };
            let nletters = puz.letters.len();
            let nwords = puz.words.len();
            Row {
                typ: e.typ.clone(),
                date: e.date.clone(),
                ms,
                consistent,
                winnable,
                ok_byte,
                ok_norm,
                verdict,
                qw_truth,
                qw_got,
                full_truth,
                full_got,
                nletters,
                nwords,
                got: if ok_norm { String::new() } else { plain },
                answer: if ok_norm { String::new() } else { e.answer.clone() },
                puzzle: if ok_norm { String::new() } else { e.puzzle.clone() },
            }
        })
        .collect();
    let wall = t_all.elapsed().as_secs_f64();
    println!("wall: {:.1}s for {} entries", wall, rows.len());

    // Summary.
    let n = rows.len();
    let ok = rows.iter().filter(|r| r.ok_norm).count();
    let noise = rows.iter().filter(|r| r.verdict == "label-noise").count();
    let search = rows.iter().filter(|r| r.verdict == "search-error").count();
    let model = rows.iter().filter(|r| r.verdict == "model-error").count();
    let tie = rows.iter().filter(|r| r.verdict == "tie").count();
    println!(
        "norm-acc: {}/{} = {:.2}%  | label-noise={} search-error={} model-error={} tie={}",
        ok,
        n,
        100.0 * ok as f64 / n as f64,
        noise,
        search,
        model,
        tie
    );
    for typ in ["cryptoquip", "cryptoquote", "celebrity-cipher"] {
        let sub: Vec<&Row> = rows.iter().filter(|r| r.typ == typ).collect();
        let o: Vec<&Row> = sub.iter().filter(|r| r.ok_norm).cloned().collect();
        println!(
            "  {:<16} {}/{} = {:.2}%",
            typ,
            o.len(),
            sub.len(),
            100.0 * o.len() as f64 / sub.len().max(1) as f64
        );
    }
    let okb = rows.iter().filter(|r| r.ok_byte).count();
    println!("byte-exact acc: {}/{} = {:.2}%", okb, n, 100.0 * okb as f64 / n as f64);

    let mut f = String::new();
    for r in &rows {
        f.push_str(&format!(
            "{{\"type\":\"{}\",\"date\":\"{}\",\"ms\":{:.1},\"consistent\":{},\"winnable\":{},\"ok_byte\":{},\"ok_norm\":{},\"verdict\":\"{}\",\"qw_truth\":{:.1},\"qw_got\":{:.1},\"full_truth\":{:.1},\"full_got\":{:.1},\"nletters\":{},\"nwords\":{},\"got\":\"{}\",\"answer\":\"{}\",\"puzzle\":\"{}\"}}\n",
            r.typ, r.date, r.ms, r.consistent, r.winnable, r.ok_byte, r.ok_norm,
            r.verdict, r.qw_truth, r.qw_got, r.full_truth, r.full_got, r.nletters, r.nwords,
            esc(&r.got), esc(&r.answer), esc(&r.puzzle)
        ));
    }
    std::fs::write(&out_path, f).unwrap();
    println!("wrote {}", out_path);
}
