//! Word bigram language model: P(w2 | w1) with Laplace smoothing.
//!
//! Data: `data/word_bigrams.bin` — top 200k bigrams from text8.
//! Layout: u32 magic 0x32475242, u32 count, f32 floor, f32 ceil,
//! then records sorted by (w1_id, w2_id): u32 w1, u32 w2, u8 qscore,
//! then f32 oov_score.

use std::sync::OnceLock;

pub struct Bigrams {
    /// Sorted (w1_id, w2_id) pairs packed as u64.
    keys: Vec<u64>,
    qs: Vec<u8>,
    floor: f32,
    scale: f32,
    oov: f32,
}

static BG: OnceLock<Bigrams> = OnceLock::new();

pub fn bigrams() -> &'static Bigrams {
    BG.get_or_init(|| {
        // WASM: compact 60k-bigram table (~300KB) with u16 word IDs into the
        // 50k vocabulary. Native: full 200k table with u32 IDs.
        #[cfg(target_arch = "wasm32")]
        {
            let b: &[u8] = include_bytes!("../../data/word_bigrams_50k.bin");
            let magic = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            assert_eq!(magic, 0x3547_5242, "word_bigrams_50k.bin bad magic");
            let count = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
            let floor = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
            let ceil = f32::from_le_bytes([b[12], b[13], b[14], b[15]]);
            let scale = (ceil - floor) / 255.0;
            let mut keys = Vec::with_capacity(count);
            let mut qs = Vec::with_capacity(count);
            let mut pos = 16usize;
            for _ in 0..count {
                let w1 = u16::from_le_bytes([b[pos], b[pos + 1]]) as u32;
                let w2 = u16::from_le_bytes([b[pos + 2], b[pos + 3]]) as u32;
                let q = b[pos + 4];
                pos += 5;
                keys.push(((w1 as u64) << 32) | (w2 as u64));
                qs.push(q);
            }
            let oov = f32::from_le_bytes([b[pos], b[pos + 1], b[pos + 2], b[pos + 3]]);
            return Bigrams {
                keys,
                qs,
                floor,
                scale,
                oov,
            };
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let b: &[u8] = include_bytes!("../../data/word_bigrams.bin");
        let magic = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        assert_eq!(magic, 0x3247_5242, "word_bigrams.bin bad magic");
        let count = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
        let floor = f32::from_le_bytes([b[8], b[9], b[10], b[11]]);
        let ceil = f32::from_le_bytes([b[12], b[13], b[14], b[15]]);
        let scale = (ceil - floor) / 255.0;
        let mut keys = Vec::with_capacity(count);
        let mut qs = Vec::with_capacity(count);
        let mut pos = 16usize;
        for _ in 0..count {
            let w1 = u32::from_le_bytes([b[pos], b[pos + 1], b[pos + 2], b[pos + 3]]);
            let w2 = u32::from_le_bytes([b[pos + 4], b[pos + 5], b[pos + 6], b[pos + 7]]);
            let q = b[pos + 8];
            pos += 9;
            keys.push(((w1 as u64) << 32) | (w2 as u64));
            qs.push(q);
        }
        let oov = f32::from_le_bytes([b[pos], b[pos + 1], b[pos + 2], b[pos + 3]]);
        Bigrams {
            keys,
            qs,
            floor,
            scale,
            oov,
        }
        } // end cfg(not(wasm32))
    })
}

impl Bigrams {
    /// Score P(w2 | w1). Returns oov if either word is unknown (id = u32::MAX)
    /// or the bigram isn't in the table.
    pub fn score_pair(&self, w1_id: u32, w2_id: u32) -> f32 {
        if w1_id == u32::MAX || w2_id == u32::MAX {
            return self.oov;
        }
        let key = ((w1_id as u64) << 32) | (w2_id as u64);
        match self.keys.binary_search(&key) {
            Ok(i) => self.floor + self.qs[i] as f32 * self.scale,
            Err(_) => self.oov,
        }
    }

    pub fn oov(&self) -> f32 {
        self.oov
    }
}
