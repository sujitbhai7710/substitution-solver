//! Canonicalization: raw puzzle text -> solver symbol sequences,
//! plus verbatim replay of every non-letter.
//!
//! HARD RULE: this module never lossy-normalizes. Curly quotes, em dashes,
//! ellipsis, digits, casing and whitespace are passed through byte-for-byte
//! on output. Only ASCII A-Z/a-z participate in substitution.

/// A slot inside a word token: either a cipher letter or an intra-word
/// apostrophe (ASCII `'` or U+2019 `'`, e.g. DON'T / THEY'RE).
#[derive(Clone, Copy, Debug)]
pub enum Slot {
    Letter(u8), // cipher letter 0..26
    Apos,
}

/// A word token for word-level scoring / pattern lookup.
/// Hyphenated compounds are split into separate tokens; apostrophes stay
/// inside the token so contractions keep their pattern (DON'T -> ABC'D).
#[derive(Clone, Debug)]
pub struct WordTok {
    pub start: u32,       // index of first letter in `Puzzle::letters`
    pub slots: Vec<Slot>, // letters + intra-word apostrophes
}

pub struct Puzzle {
    pub raw: String,
    pub letters: Vec<u8>,              // cipher letters 0..26, in order
    /// Symbol stream over the raw text: 0..26 = cipher letter, 26 = any
    /// non-letter (space, punctuation, digit, ...). One entry per char.
    /// The 27-symbol quadgram model scores over this stream so word
    /// boundaries inform the search.
    pub text: Vec<u8>,
    pub pos_of_letter: [Vec<u32>; 26], // positions in `text`, per cipher letter
    pub words: Vec<WordTok>,
    pub words_of_letter: [Vec<u32>; 26], // word indices containing each cipher letter
}

fn is_apos(ch: char) -> bool {
    ch == '\'' || ch == '\u{2019}'
}

fn flush_token(cur: &mut Vec<Slot>, cur_start: u32, words: &mut Vec<WordTok>) {
    // A trailing apostrophe (e.g. a closing quote ') is not part of the word.
    while matches!(cur.last(), Some(Slot::Apos)) {
        cur.pop();
    }
    if !cur.is_empty() {
        words.push(WordTok {
            start: cur_start,
            slots: std::mem::take(cur),
        });
    } else {
        cur.clear();
    }
}

/// Parse raw puzzle text. Never normalizes: the original string is kept
/// verbatim for replay.
pub fn parse(raw: &str) -> Puzzle {
    let mut letters: Vec<u8> = Vec::new();
    let mut text: Vec<u8> = Vec::new();
    let mut words: Vec<WordTok> = Vec::new();
    let mut cur: Vec<Slot> = Vec::new();
    let mut cur_start: u32 = 0;

    for ch in raw.chars() {
        if ch.is_ascii_alphabetic() {
            if cur.is_empty() {
                cur_start = letters.len() as u32;
            }
            let c = ch.to_ascii_uppercase() as u8 - b'A';
            cur.push(Slot::Letter(c));
            letters.push(c);
            text.push(c);
        } else if is_apos(ch) && !cur.is_empty() {
            // Intra-word apostrophe joins the token (DON'T, THEY'RE).
            cur.push(Slot::Apos);
            text.push(26);
        } else {
            // Any other character (space, hyphen, punctuation, digit,
            // newline, Unicode) ends the token and passes through verbatim.
            if !cur.is_empty() {
                flush_token(&mut cur, cur_start, &mut words);
            }
            text.push(26);
        }
    }
    if !cur.is_empty() {
        flush_token(&mut cur, cur_start, &mut words);
    }

    let mut pos_of_letter: [Vec<u32>; 26] = Default::default();
    for (i, &c) in text.iter().enumerate() {
        if c < 26 {
            pos_of_letter[c as usize].push(i as u32);
        }
    }
    let mut words_of_letter: [Vec<u32>; 26] = Default::default();
    for (wi, w) in words.iter().enumerate() {
        let mut seen = [false; 26];
        for s in &w.slots {
            if let Slot::Letter(c) = s {
                seen[*c as usize] = true;
            }
        }
        for (c, wol) in words_of_letter.iter_mut().enumerate() {
            if seen[c] {
                wol.push(wi as u32);
            }
        }
    }

    Puzzle {
        raw: raw.to_string(),
        letters,
        text,
        pos_of_letter,
        words,
        words_of_letter,
    }
}

/// Parse a clue / crib string into (cipher, plain) pairs.
/// Accepts "B=M", "B = M", "B=M,G=R", "QVW=THE" (multi-letter cribs).
pub fn parse_clue(s: &str) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    for chunk in s.to_ascii_uppercase().split([',', ';', '\n']) {
        let mut it = chunk.split('=');
        let (Some(a), Some(b), None) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        let cs: Vec<u8> = a
            .bytes()
            .filter(|b| b.is_ascii_alphabetic())
            .map(|b| b - b'A')
            .collect();
        let ps: Vec<u8> = b
            .bytes()
            .filter(|b| b.is_ascii_alphabetic())
            .map(|b| b - b'A')
            .collect();
        if cs.len() == ps.len() && !cs.is_empty() {
            out.extend(cs.into_iter().zip(ps.into_iter()));
        }
    }
    out
}

/// Decrypt: substitute ASCII letters via `key` (cipher -> plain), preserving
/// the original case; every other character is emitted verbatim.
pub fn replay(raw: &str, key: &[u8; 26]) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphabetic() {
            let c = ch.to_ascii_uppercase() as u8 - b'A';
            let p = key[c as usize];
            out.push(if ch.is_ascii_uppercase() {
                (b'A' + p) as char
            } else {
                (b'a' + p) as char
            });
        } else {
            out.push(ch);
        }
    }
    out
}

/// Exact-match test: letters compare case-insensitively, everything else
/// must be byte-identical (punctuation / Unicode exactness).
pub fn exact_match(out: &str, expected: &str) -> bool {
    let mut a = out.chars();
    let mut b = expected.chars();
    loop {
        match (a.next(), b.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) => {
                if x.is_ascii_alphabetic() && y.is_ascii_alphabetic() {
                    if !x.eq_ignore_ascii_case(&y) {
                        return false;
                    }
                } else if x != y {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

/// Check puzzle<->answer substitution consistency: the letter alignment must
/// define a bijective map. Used to filter the benchmark corpus.
pub fn substitution_consistent(puzzle: &str, answer: &str) -> bool {
    let lp: Vec<u8> = puzzle
        .bytes()
        .filter(|b| b.is_ascii_alphabetic())
        .map(|b| b.to_ascii_uppercase() - b'A')
        .collect();
    let la: Vec<u8> = answer
        .bytes()
        .filter(|b| b.is_ascii_alphabetic())
        .map(|b| b.to_ascii_uppercase() - b'A')
        .collect();
    if lp.len() != la.len() || lp.is_empty() {
        return false;
    }
    let mut c2p = [-1i8; 26];
    let mut p2c = [-1i8; 26];
    for (&c, &p) in lp.iter().zip(la.iter()) {
        let (c, p) = (c as usize, p as usize);
        if c2p[c] == -1 {
            c2p[c] = p as i8;
        } else if c2p[c] != p as i8 {
            return false;
        }
        if p2c[p] == -1 {
            p2c[p] = c as i8;
        } else if p2c[p] != c as i8 {
            return false;
        }
    }
    true
}

/// Entries on which an exact match is achievable: letter-consistent AND every
/// non-letter byte identical (a solver that replays puzzle punctuation
/// verbatim can never match an answer whose punctuation differs).
pub fn exact_winnable(puzzle: &str, answer: &str) -> bool {
    if !substitution_consistent(puzzle, answer) {
        return false;
    }
    let strip: Vec<u8> = puzzle
        .bytes()
        .filter(|b| !b.is_ascii_alphabetic())
        .collect();
    let strip_a: Vec<u8> = answer
        .bytes()
        .filter(|b| !b.is_ascii_alphabetic())
        .collect();
    strip == strip_a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_preserves_unicode_punct() {
        let key = core::array::from_fn(|i| i as u8); // identity
        let raw = "“HELLO,” — SHE SAID. THEY’RE 2 GO.";
        assert_eq!(replay(raw, &key), raw);
    }

    #[test]
    fn parse_keeps_apostrophe_in_token() {
        let p = parse("THEY’RE DON'T BULL-DOSERS");
        assert_eq!(p.words.len(), 4); // THEY'RE, DON'T, BULL, DOSERS
        assert_eq!(p.letters.len(), 6 + 4 + 4 + 6);
    }

    #[test]
    fn clue_parse() {
        assert_eq!(parse_clue("B=M"), vec![(1, 12)]);
        assert_eq!(parse_clue("QVW=THE").len(), 3);
    }

    /// Frontend regression: curly apostrophe (U+2019) must survive replay.
    /// The old frontend normalized ’ to ', breaking exact-match.
    #[test]
    fn replay_preserves_curly_apostrophe() {
        let key = core::array::from_fn(|i| i as u8);
        let raw = "DON’T STOP ’TIL YOU’RE DONE";
        assert_eq!(replay(raw, &key), raw);
        // And parse must keep it inside the token.
        let p = parse(raw);
        assert_eq!(p.words.len(), 5);
    }

    /// Repeated spaces, tabs, and line breaks pass through verbatim.
    #[test]
    fn replay_preserves_whitespace_variants() {
        let key = core::array::from_fn(|i| i as u8);
        let raw = "HELLO  WORLD\tTABS\nNEWLINES\r\nCRLF   END";
        assert_eq!(replay(raw, &key), raw);
    }

    /// All dash variants are preserved (never normalized to hyphen).
    #[test]
    fn replay_preserves_dash_variants() {
        let key = core::array::from_fn(|i| i as u8);
        // hyphen, en dash, em dash, minus sign, non-breaking hyphen
        let raw = "A-B–C—D−E‑F";
        assert_eq!(replay(raw, &key), raw);
    }

    /// Mixed casing is preserved (uppercase stays upper, lower stays lower).
    #[test]
    fn replay_preserves_casing() {
        // Key maps A->B, B->C, ..., Z->A (shift by 1).
        let key: [u8; 26] = core::array::from_fn(|i| ((i + 1) % 26) as u8);
        let raw = "Hello World";
        // H->I, e->f, l->m, o->p, W->X, r->s, d->e
        assert_eq!(replay(raw, &key), "Ifmmp Xpsme");
    }

    /// Digits pass through untouched (never participate in substitution).
    #[test]
    fn replay_preserves_digits() {
        let key = core::array::from_fn(|i| i as u8);
        let raw = "CODE 123: ABC 456 XYZ 7890";
        assert_eq!(replay(raw, &key), raw);
        // Digits are not letters: they don't enter the puzzle's letter stream.
        let p = parse(raw);
        // CODE(4) + ABC(3) + XYZ(3) = 10 letters; digits excluded.
        assert_eq!(p.letters.len(), 10);
    }

    /// Non-letter bytes must match exactly for exact_winnable.
    #[test]
    fn exact_winnable_rejects_punct_differences() {
        // Same letters, different dash: not winnable.
        assert!(!exact_winnable("A–B", "A-B"));
        // Identical: winnable.
        assert!(exact_winnable("A–B", "A–B"));
    }
}
