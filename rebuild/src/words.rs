//! Word-frequency data: unigram log-probs for the word-bonus scorer and a
//! pattern index (HELLO -> ABCCD) for the dictionary-attack seed.
//! Built once at init from the embedded wordlist; general English data only.

use std::collections::HashMap;
use std::sync::OnceLock;

pub struct WordData {
    /// Words in frequency order (index 0 = most frequent).
    pub words: Vec<Vec<u8>>,
    /// ln probability per word (add-alpha smoothed).
    pub logp: Vec<f32>,
    /// ln probability for out-of-vocabulary tokens.
    pub oov: f32,
    pub by_word: HashMap<Vec<u8>, u32>,
    /// pattern bytes -> word indices in frequency order.
    pub patterns: HashMap<Vec<u8>, Vec<u32>>,
}

/// Canonical letter pattern: HELLO -> ABCCD, DON'T -> ABC'D.
/// Apostrophes are kept literal so contractions only match contractions.
pub fn pattern_of(w: &[u8]) -> Vec<u8> {
    let mut map = [0u8; 256];
    let mut next = b'A';
    let mut out = Vec::with_capacity(w.len());
    for &ch in w {
        if ch == b'\'' {
            out.push(b'\'');
            continue;
        }
        if map[ch as usize] == 0 {
            map[ch as usize] = next;
            next = next.wrapping_add(1);
        }
        out.push(map[ch as usize]);
    }
    out
}

static DATA: OnceLock<WordData> = OnceLock::new();

pub fn word_data() -> &'static WordData {
    DATA.get_or_init(|| {
        // WASM: use the 50k-word list to meet the 1.26MB size target.
        // Native: use the full list (or WORDLIST_LIMIT for experiments).
        #[cfg(target_arch = "wasm32")]
        let b: &[u8] = include_bytes!("../../data/words_50k.bin");
        #[cfg(not(target_arch = "wasm32"))]
        let b: &[u8] = include_bytes!("../../data/words.bin");
        // WORDLIST_LIMIT (dev/benchmark only): truncate to top-N by frequency
        // to measure the size/accuracy tradeoff. (words.bin is already
        // frequency-ordered, so this just takes a prefix.)
        let limit: usize = std::env::var("WORDLIST_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(usize::MAX);
        let magic = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        assert_eq!(magic, 0x5752_4431, "words.bin bad magic");
        let count = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
        let oov = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
        let qfloor = f32::from_le_bytes([b[12], b[13], b[14], b[15]]);
        let qscale = f32::from_le_bytes([b[16], b[17], b[18], b[19]]);
        let count = count.min(limit);
        let mut words: Vec<Vec<u8>> = Vec::with_capacity(count);
        let mut logp: Vec<f32> = Vec::with_capacity(count);
        let mut pos = 20usize;
        for _ in 0..count {
            let len = b[pos] as usize;
            pos += 1;
            let wb = b[pos..pos + len].to_vec();
            pos += len;
            let q = b[pos] as f32;
            pos += 1;
            words.push(wb);
            logp.push(qfloor + q * qscale);
        }
        assert!(words.len() > 1000, "wordlist too small");

        let mut by_word: HashMap<Vec<u8>, u32> = HashMap::with_capacity(words.len());
        for (i, w) in words.iter().enumerate() {
            by_word.entry(w.clone()).or_insert(i as u32);
        }
        let mut patterns: HashMap<Vec<u8>, Vec<u32>> = HashMap::new();
        for (i, w) in words.iter().enumerate() {
            patterns.entry(pattern_of(w)).or_default().push(i as u32);
        }

        WordData {
            words,
            logp,
            oov,
            by_word,
            patterns,
        }
    })
}

/// Score one decoded word token (uppercase bytes, apostrophes kept).
/// Possessives fall back to the stem ("NOAH'S" -> "NOAH") with a penalty;
/// anything else unseen gets the OOV penalty. General rule, no special cases.
pub fn score_word(wd: &WordData, tok: &[u8]) -> f32 {
    if let Some(&i) = wd.by_word.get(tok) {
        return wd.logp[i as usize];
    }
    let n = tok.len();
    if n > 2 && tok[n - 2] == b'\'' && tok[n - 1] == b'S' {
        if let Some(&i) = wd.by_word.get(&tok[..n - 2]) {
            return wd.logp[i as usize] - 1.0;
        }
    }
    wd.oov
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        assert_eq!(pattern_of(b"HELLO"), b"ABCCD");
        assert_eq!(pattern_of(b"DON'T"), b"ABC'D");
        assert_eq!(pattern_of(b"NOON"), b"ABBA");
    }
}

