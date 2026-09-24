//! Character trigram language model over a 27-symbol alphabet (A-Z + space).
//!
//! Data: `data/trigrams.bin` — trigram log-probs from text8, add-alpha
//! smoothed, quantized to u8 (~20 KB total).
//! Layout: u32 magic 0x54524731 ("TRG1"), f32 floor, f32 scale,
//! then 27^3 u8 entries, index = ((a*27+b)*27+c).
//!
//! The space symbol is what makes this model valuable for re-ranking: it
//! scores word boundaries, which letter-only quadgrams cannot see. This is
//! the "trigram posterior including spaces" from Olson 2007.

use std::sync::OnceLock;

pub struct Trigrams {
    table: Vec<f32>, // 19683 entries
}

static TG: OnceLock<Trigrams> = OnceLock::new();

pub fn trigrams() -> &'static Trigrams {
    TG.get_or_init(|| {
        let b: &[u8] = include_bytes!("../../data/trigrams.bin");
        let magic = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        assert_eq!(magic, 0x5452_4731, "trigrams.bin bad magic");
        let floor = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        let scale = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
        assert_eq!(b.len(), 12 + 19_683, "trigrams.bin has wrong size");
        let table: Vec<f32> = b[12..].iter().map(|&q| floor + q as f32 * scale).collect();
        Trigrams { table }
    })
}

impl Trigrams {
    /// Score decoded text: uppercase ASCII bytes; any non-letter is treated
    /// as a word boundary (space symbol). Lowercase is uppercased.
    pub fn score_bytes(&self, text: &[u8]) -> f32 {
        let n = text.len();
        if n < 3 {
            return 0.0;
        }
        // Map to 27 symbols on the fly.
        let sym = |c: u8| -> usize {
            if c.is_ascii_uppercase() {
                (c - b'A') as usize
            } else if c.is_ascii_lowercase() {
                (c - b'a') as usize
            } else {
                26
            }
        };
        let mut total = 0.0f32;
        let (mut a, mut b) = (sym(text[0]), sym(text[1]));
        for &c in &text[2..] {
            let cc = sym(c);
            total += self.table[(a * 27 + b) * 27 + cc];
            a = b;
            b = cc;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_beats_xqz() {
        let tg = trigrams();
        assert!(tg.score_bytes(b" THE ") > tg.score_bytes(b" XQZ "));
    }
}
