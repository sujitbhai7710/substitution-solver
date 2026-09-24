//! Word-bigram + character-trigram re-ranking of search candidates.
//!
//! The quadgram + word-unigram model drives the simulated annealing search,
//! but it can prefer a plausible wrong plaintext (e.g. LIKE over LIFE) when
//! the local n-gram statistics favor it. We re-rank the distinct candidates
//! (restart winners + dictionary-attack hits) by
//!   combined = orig_score + bigram_w * bigram_score + trigram_w * trigram_score
//! and return the best. The word bigram P(w2|w1) captures collocations like
//! "LIFE WILL" vs "LIKE WILL"; the 27-symbol character trigram (letters +
//! space) scores word boundaries, which letter-only quadgrams cannot see.

use crate::bigrams::bigrams;
use crate::codec::{replay, Puzzle};
use crate::search::{Config, Solution};
use crate::trigrams::trigrams;
use crate::words::word_data;

/// Combined final-selection score for one candidate.
fn combined_score(puz: &Puzzle, w: &Solution, cfg: &Config) -> f64 {
    if cfg.bigram_w == 0.0 && cfg.trigram_w == 0.0 && cfg.name_w == 0.0 {
        return w.score;
    }
    let text = replay(&puz.raw, &w.key);
    let tb = text.as_bytes();
    let bg_score = if cfg.bigram_w != 0.0 {
        score_bigrams(tb, bigrams(), word_data()) as f64
    } else {
        0.0
    };
    let tg_score = if cfg.trigram_w != 0.0 {
        trigrams().score_bytes(tb) as f64
    } else {
        0.0
    };
    let name_score = if cfg.name_w != 0.0 {
        crate::gazetteer::name_bonus(tb) as f64
    } else {
        0.0
    };
    w.score + cfg.bigram_w * bg_score + cfg.trigram_w * tg_score + cfg.name_w * name_score
}

/// Re-rank all candidates by the combined final-selection score, best first.
/// Only the top few by original score are re-ranked; the tail is noise.
pub fn rerank_all(puz: &Puzzle, winners: &[Solution], cfg: &Config) -> Vec<Solution> {
    assert!(!winners.is_empty());
    if winners.len() == 1 {
        return vec![winners[0].clone()];
    }
    // Only re-rank the top few by original score; the tail is noise.
    let k = winners.len().min(12);
    let mut ranked: Vec<Solution> = winners
        .iter()
        .take(k)
        .map(|w| {
            let mut s = w.clone();
            s.score = combined_score(puz, w, cfg);
            s
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked
}

/// Re-rank candidates; return the best combined.
pub fn rerank_best(puz: &Puzzle, winners: &[Solution], cfg: &Config) -> Solution {
    rerank_all(puz, winners, cfg).into_iter().next().unwrap()
}

/// Sum of P(w2|w1) log-probs over adjacent word pairs in the text.
/// Words are uppercase A-Z sequences; anything else is a separator.
fn score_bigrams(
    text: &[u8],
    bg: &crate::bigrams::Bigrams,
    wd: &crate::words::WordData,
) -> f32 {
    let mut total = 0.0f32;
    let mut prev_id: Option<u32> = None;
    let mut cur = Vec::with_capacity(16);
    // Iterate with a trailing sentinel separator to flush the last word.
    for &c in text.iter().chain(std::iter::once(&b' ')) {
        if c.is_ascii_alphabetic() {
            cur.push(c.to_ascii_uppercase());
        } else if !cur.is_empty() {
            let id = wd.by_word.get(&cur).copied().unwrap_or(u32::MAX);
            if let Some(p) = prev_id {
                total += bg.score_pair(p, id);
            }
            prev_id = Some(id);
            cur.clear();
        }
        // Consecutive separators: no word boundary event (prev stays).
    }
    total
}
