//! Public-figure name gazetteer for the final correction pass.
//!
//! Source: GIPHY celebrity-detection open-source label set (2,306 names,
//! public dataset). Used ONLY in the final correction pass to propose
//! candidates for out-of-vocabulary tokens (especially attribution text).
//! Never used in core search scoring, so it cannot pollute the main solve.
//!
//! General knowledge only: no puzzle answers, no corpus-derived names.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::words::pattern_of;

/// Embedded names, one per line, parts separated by space, uppercase.
/// (data/celebs.txt, 2,306 names from the public GIPHY label set.)
const NAMES: &str = include_str!("../../data/celebs.txt");

/// pattern bytes -> gazetteer name parts with that letter pattern.
static MAP: OnceLock<HashMap<Vec<u8>, Vec<Vec<u8>>>> = OnceLock::new();

fn gaz_map() -> &'static HashMap<Vec<u8>, Vec<Vec<u8>>> {
    MAP.get_or_init(|| {
        let mut m: HashMap<Vec<u8>, Vec<Vec<u8>>> = HashMap::new();
        let mut seen: HashMap<Vec<u8>, ()> = HashMap::new();
        for line in NAMES.lines() {
            for part in line.split(' ') {
                let pb = part.as_bytes();
                if pb.is_empty() {
                    continue;
                }
                // Skip non-letter parts (e.g. "50" from "50 CENT").
                if !pb.iter().all(|b| b.is_ascii_uppercase()) {
                    continue;
                }
                // Deduplicate parts (e.g. "JOHN" appears in many names).
                if seen.contains_key(pb) {
                    continue;
                }
                seen.insert(pb.to_vec(), ());
                let pat = pattern_of(pb);
                m.entry(pat).or_default().push(pb.to_vec());
            }
        }
        m
    })
}

/// All distinct gazetteer name parts with the given letter pattern.
/// Returns an empty slice when no name part has that pattern.
pub fn candidates_for_pattern(pat: &[u8]) -> &'static [Vec<u8>] {
    match gaz_map().get(pat) {
        Some(v) => v.as_slice(),
        None => &[],
    }
}

/// Number of distinct name parts in the gazetteer (diagnostic).
#[allow(dead_code)]
pub fn part_count() -> usize {
    gaz_map().values().map(|v| v.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gazetteer_loads() {
        // Sanity: thousands of distinct parts loaded.
        assert!(part_count() > 2000, "gazetteer too small: {}", part_count());
    }

    #[test]
    fn pattern_lookup_finds_known_names() {
        // AZIZ -> pattern ABAB; the GIPHY set contains AZIZ ANSARI.
        let pat = pattern_of(b"AZIZ");
        let cands = candidates_for_pattern(&pat);
        assert!(
            cands.iter().any(|c| c == b"AZIZ"),
            "AZIZ not found in gazetteer"
        );
        // ZENDAYA should be present as a single part.
        let pat = pattern_of(b"ZENDAYA");
        let cands = candidates_for_pattern(&pat);
        assert!(
            cands.iter().any(|c| c == b"ZENDAYA"),
            "ZENDAYA not found in gazetteer"
        );
    }

    #[test]
    fn no_digit_parts() {
        // "50 CENT" contributes CENT but not "50".
        for parts in gaz_map().values() {
            for p in parts {
                assert!(
                    p.iter().all(|b| b.is_ascii_uppercase()),
                    "non-letter part in gazetteer: {:?}",
                    String::from_utf8_lossy(p)
                );
            }
        }
    }
}

/// Full multi-word names as space-joined uppercase byte strings.
static FULL_NAMES: OnceLock<HashSet<Vec<u8>>> = OnceLock::new();

fn full_names() -> &'static HashSet<Vec<u8>> {
    FULL_NAMES.get_or_init(|| {
        let mut s = HashSet::new();
        for line in NAMES.lines() {
            let parts: Vec<&str> = line
                .split(' ')
                .filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_uppercase()))
                .collect();
            if parts.len() >= 2 {
                s.insert(parts.join(" ").into_bytes());
            }
        }
        s
    })
}

/// Count occurrences of known public-figure full names (2-4 words) in
/// decoded text. Used as a small bonus in final selection: attributions
/// like "AMY POEHLER" or "LEBRON JAMES" are strong evidence for a key.
/// Greedy longest-match so overlapping names don't double-count.
pub fn name_bonus(text: &[u8]) -> f32 {
    let set = full_names();
    if set.is_empty() {
        return 0.0;
    }
    // Split into uppercase words.
    let mut words: Vec<Vec<u8>> = Vec::new();
    let mut cur = Vec::new();
    for &c in text.iter().chain(std::iter::once(&b' ')) {
        if c.is_ascii_alphabetic() {
            cur.push(c.to_ascii_uppercase());
        } else if !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
    }
    let mut count = 0u32;
    let mut i = 0;
    let mut buf = Vec::with_capacity(48);
    while i < words.len() {
        let mut matched = 0usize;
        for len in (2..=4).rev() {
            if i + len > words.len() {
                continue;
            }
            buf.clear();
            for (k, w) in words[i..i + len].iter().enumerate() {
                if k > 0 {
                    buf.push(b' ');
                }
                buf.extend_from_slice(w);
            }
            if set.contains(&buf) {
                matched = len;
                break;
            }
        }
        if matched > 0 {
            count += 1;
            i += matched;
        } else {
            i += 1;
        }
    }
    count as f32
}

#[cfg(test)]
mod name_tests {
    use super::*;
    #[test]
    fn finds_full_name() {
        assert!(name_bonus(b"said AMY POEHLER loudly") > 0.0);
        assert_eq!(name_bonus(b"nothing here at all"), 0.0);
    }
}
