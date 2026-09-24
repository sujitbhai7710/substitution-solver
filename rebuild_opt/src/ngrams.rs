//! Quadgram log-probability table over 27 symbols (A-Z + space = 26).
//!
//! Data: `data/quadgrams27.q8` — counted from text8 (Wikipedia), quantized
//! to u8 (~531 KB). Layout: u32 magic 0x51444732 ("QDG2"), f32 floor,
//! f32 scale, then 27^4 = 531441 u8 entries,
//! index = ((a*27+b)*27+c)*27+d. Embedded at compile time so WASM
//! instantiation has zero data-load cost.
//!
//! The space symbol is the key upgrade over the old 26-letter table: the
//! model scores word boundaries (e.g. "E␣CA" vs jammed "ECA"), which is
//! where short cryptograms are won or lost.

use std::sync::OnceLock;

static TABLE: OnceLock<Vec<f32>> = OnceLock::new();

/// Natural-log probabilities, index = ((a*27+b)*27+c)*27+d, symbols 0..27
/// (26 = space / non-letter).
pub fn quad_table() -> &'static [f32] {
    TABLE.get_or_init(|| {
        let b: &[u8] = include_bytes!("../../data/quadgrams27.q8");
        let magic = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        assert_eq!(magic, 0x5144_4732, "quadgrams27.q8 bad magic");
        let floor = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        let scale = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
        assert_eq!(b.len(), 12 + 531_441, "quadgrams27.q8 has wrong size");
        b[12..].iter().map(|&q| floor + q as f32 * scale).collect()
    })
}

#[inline(always)]
pub fn quad_index(a: u8, b: u8, c: u8, d: u8) -> usize {
    ((a as usize * 27 + b as usize) * 27 + c as usize) * 27 + d as usize
}
