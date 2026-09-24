# MISTAKES.md — Solver Failure Analysis and Fixes

**Date:** 2026-09-24
**Scope:** Held-out test corpus (`test.json`, 1,035 puzzles) + tuning dev set (`dev.json`, 209 puzzles).
**Scoring:** Lowercase ASCII letters-only full-string equality.
**Constraint honored:** No test answers hardcoded; no model/feature derived from held-out labels. All fixes are algorithmic.

---

## 1. Headline numbers

### Dev set (209 puzzles — tuning)

| Config | Overall | Cryptoquip (98) | Cryptoquote (45) | Celeb. Cipher (66) |
|---|---|---|---|---|
| Baseline: 48×12,000, pre-fix | 181/209 = **86.60%** | 92/98 = 93.88% | 30/45 = 66.67% | 59/66 = 89.39% |
| + pin-scale bug fix | 184/209 = **88.04%** (+3) | 94/98 = 95.92% | 30/45 = 66.67% | 60/66 = 90.91% |
| + 96×6,000 restarts (same 576k-step budget) | 187/209 = **89.47%** (+3 net) | 95/98 = 96.94% | 33/45 = 73.33% | 59/66 = 89.39% |

### Held-out test set (1,035 puzzles — validation)

| Config | Overall | Cryptoquip (487) | Cryptoquote (220) | Celeb. Cipher (328) |
|---|---|---|---|---|
| Baseline: 48×12,000, pre-fix | 873/1035 = **84.35%** | 455/487 = 93.44% | 152/220 = 69.09% | 266/328 = 81.10% |
| + pin fix + 96×6,000 | 897/1035 = **86.67%** (+24, +2.32 pp) | 470/487 = 96.51% (+15) | 159/220 = 72.27% (+7) | 268/328 = 81.71% (+2) |

Verdict distribution on test (baseline → fixed): search-error 85 → 59 (−26), model-error 76 → 78 (+2), tie 1 → 1, **label-noise 0 → 0**.

Median solve time ~3.0 s, max ~9.0 s (production 48×12,000 config, 2 workers). The 96×6,000 config runs in the same time envelope (identical step budget).

---

## 2. Methodology — are the "mistakes" real?

Every miss was triaged mechanically (no hand-waving):

- **Label-noise check:** (a) byte-exact vs letters-normalized scoring, (b) `substitution_consistent` — does the label decode the puzzle under a single consistent key, (c) `exact_winnable` — is the label's implied key bijective. **Result: 0 label-noise cases on both dev and test.** All labels are clean, consistent, winnable.
- **Search-error vs model-error:** reconstruct the label's true key (only for consistent labels), score it under the solver's *actual* final selection objective (`full_score` = quadgram + word-unigram + word-bigram + word-trigram + name bonus, matching the rerank/pipeline scales). If `full(truth) > full(output)` the optimizer failed → **search-error**. If `full(output) ≥ full(truth)` the objective itself prefers the wrong answer → **model-error**. One exact tie (`cryptoquote 2026-08-08`: "C. DAY LEWIS" vs "Z. DAY LEWIS" — an unconstrained single initial, unknowable) is reported as **tie**.
- **Existing behavior that works:** the annealer + greedy polish + rerank pipeline already solves 84–87%; the dictionary pattern-seed, clue locking, and bigram polish are all sound and were left intact.

---

## 3. Fix 1 — `pin_single_ai` objective-scale bug (genuine bug, +3 dev / 0 regressions)

**Mechanism.** The pipeline is: anneal (objective `score_key` = quadgram + word-unigram) → greedy polish → `rerank_best`, which *replaces* `Solution.score` with the combined rerank scale (quad + word + bigram + trigram + name, far more negative) → `pin_single_ai`, which initialized `best_score = sol.score` (combined scale) but scored A/I-pin candidates with `score_key` (quad+word only). Comparing across incompatible scales meant almost any pin "improved" the score, corrupting correct keys; `bigram_polish` then hill-climbed from the damaged key into a plausible-but-wrong answer.

**Concrete trace** (cryptoquip `2026-01-21`): the annealer found the true key (`IF COUSTEAU HOSTED A FAIRLY RISQUE RADIO PROGRAM…`, `score_key = −1398.4`); rerank correctly selected it but stored combined score `−2579.9`; a destructive E→I pin scored `−1930.6` under `score_key` and was accepted (`−1930.6 > −2579.9`); final output became `IS COUFTEAU HOFTED A SAIRLY RIFQUE…`. From that wrong key a single swap improved `score_key` by **+383.3**, proving post-rerank corruption.

**Fix** (`src/search.rs`, `pin_single_ai`): `let mut best_score = score_key(puz, &best.key, cfg.word_w);` — baseline and candidates now share one scale. Dev: 181 → 184, zero regressions. Fixed: `cryptoquip 2026-01-21`, `cryptoquip 2025-03-04`, `celebrity-cipher 2025-12-13` (all were search-errors).

---

## 4. Fix 2 — 96 restarts × 6,000 steps instead of 48 × 12,000 (same budget, +3 net dev)

**Mechanism.** The remaining catastrophic search-errors (truth 60–330 points better under the final objective, yet never produced) were *coverage* failures, not landscape failures: with 200 restarts the annealer finds the truth basin (verified on `cryptoquip 2026-05-23`), but 48 random starts miss it. Root cause of narrow basins: pun/OOV-heavy puzzles (e.g. `DRANO`/`BRAINO`) — the dictionary pattern-seed structurally cannot seed them (`dict_attack` needs a *complete* dictionary assignment; `DRANO` ranks 15,182nd in its pattern class beyond the 2,000 candidate cap, `BRAINO` is OOV, and mid-frequency true words like `CLEARING`/`WRONG` rank ~200, beyond the DFS `expand_cap = 120`). At a fixed 576k-step budget, **coverage beats refinement**: 96 diverse starts × 6,000 steps each finds narrow basins the 48×12,000 schedule misses, while the greedy polish still refines each start. Solve time is unchanged (same step budget).

**Evidence** (7 catastrophic dev failures, 48×12k → 96×6k): fixed `cryptoquip 2026-05-23`, `cryptoquip 2026-09-07`, `cryptoquote 2025-10-14`, `cryptoquote 2025-11-20` (0/7 → 4/7). Full dev: 184 → 187 (+5 fixed, 2 regressed; the 2 regressions come from rerank *selection* among the larger candidate pool, not lost coverage — the 96-set is a strict superset of the 48-set under the fixed RNG seed). Test: search-errors 85 → 59.

**Recommendation:** adopt 96×6,000 as the production schedule (identical compute, +2.3 pp on held-out test).

---

## 5. What was tried but did NOT help (reported for completeness)

- **Separate selection word-weight (`sel_word_w`)**: hypothesis was that word-unigram weight 3.0 (needed for annealing basin-finding) overrides bigram context in final selection (e.g. `HOME` beats `LOVE` by 5.6 on word score vs +3.5 bigram for `LOVE SONG`). Implemented cleanly, tuned 1.0–3.0 on dev: **no accuracy change** (187/209 at 1.5 and 3.0; verdicts only reclassified). The model errors are robust to word weight — the wrong answer wins on quadgram/bigram, not just unigram. Reverted to keep the diff minimal.
- **Hotter annealing (`t0=20`)**: did not beat `t0=10` on the catastrophic set.
- **Bigger dict-attack node budget**: the DFS failure on OOV/pun puzzles is structural (no complete dictionary assignment exists), not a budget issue.

---

## 6. Failure pattern analysis (fixed config, test set: 138 misses)

**By category:** 59 search-error, 78 model-error, 1 tie, 0 label-noise.

**Length effect:** shortest decile (34–67 letters) 63.1%; all longer deciles 81–93%. Short puzzles remain the hardest (less statistical signal) — unchanged by these fixes.

**Cryptoquote is the weak type** (72.3% vs 96.5% cryptoquip): literary quotes have rarer diction and longer attributions; both search (31) and model (29) errors concentrate here.

**Recurring model-error confusions** (word-unigram frequency overriding context; bigram too sparse — discriminating pairs often both fall to the unseen score −12.72):
`THING→THINK`, `LIVE→LIKE`/`LIVE→LIFE`, `HOME→LOVE`, `BOY→JOY`, `AFAR→AJAR`, `FLOW→BLOW`, `THOSE→SMOKE`, `MODEL→NOVEL`, `IS→AM`, `A→I`, `FOU→YOU` (search), `BY→MY` (search).

**Name errors** (gazetteer 2,306 GIPHY names; OOV penalty −27.8 vs name bonus +10): `LIP→LIV TYLER` (in gazetteer, bonus insufficient), `PAK→PAZ VEGA`, `GAMES→JAMES BALDWIN` (both missing from gazetteer). Gazetteer expansion was *not* done — it requires a legitimate general external name list, and none was sourced in this pass. This is the clearest remaining model-error lever.

**Search-error residue** (59): mostly single-letter near-misses (`FOU→YOU`, `BY→MY`) and short-puzzle traps; the catastrophic multi-word garbage outputs are largely fixed.

---

## 7. Exact failure IDs (fixed config: pin fix + 96×6,000, test set)

Format: `type date category`. All 138 misses; 0 are label noise.

### search-error (59)
celebrity-cipher 2025-01-08 · celebrity-cipher 2025-02-06 · celebrity-cipher 2025-03-10 · celebrity-cipher 2025-05-03 · celebrity-cipher 2025-07-16 · celebrity-cipher 2025-07-23 · celebrity-cipher 2025-10-07 · celebrity-cipher 2025-10-17 · celebrity-cipher 2025-10-30 · celebrity-cipher 2025-11-01 · celebrity-cipher 2025-12-02 · celebrity-cipher 2025-12-23 · celebrity-cipher 2026-01-10 · celebrity-cipher 2026-01-13 · celebrity-cipher 2026-01-17 · celebrity-cipher 2026-01-29 · celebrity-cipher 2026-06-17 · celebrity-cipher 2026-08-05 · celebrity-cipher 2026-08-19 · celebrity-cipher 2026-09-05 · celebrity-cipher 2026-09-10 · cryptoquip 2025-10-15 · cryptoquip 2025-10-17 · cryptoquip 2025-11-29 · cryptoquip 2025-12-06 · cryptoquip 2026-01-06 · cryptoquip 2026-02-14 · cryptoquip 2026-07-07 · cryptoquote 2025-01-13 · cryptoquote 2025-02-10 · cryptoquote 2025-02-27 · cryptoquote 2025-03-04 · cryptoquote 2025-03-13 · cryptoquote 2025-04-01 · cryptoquote 2025-07-17 · cryptoquote 2025-09-01 · cryptoquote 2025-10-09 · cryptoquote 2025-10-10 · cryptoquote 2025-10-16 · cryptoquote 2025-11-01 · cryptoquote 2025-11-06 · cryptoquote 2025-12-10 · cryptoquote 2025-12-29 · cryptoquote 2026-01-05 · cryptoquote 2026-01-20 · cryptoquote 2026-02-05 · cryptoquote 2026-03-02 · cryptoquote 2026-03-23 · cryptoquote 2026-03-30 · cryptoquote 2026-04-02 · cryptoquote 2026-04-13 · cryptoquote 2026-04-23 · cryptoquote 2026-05-07 · cryptoquote 2026-07-07 · cryptoquote 2026-07-28 · cryptoquote 2026-08-13 · cryptoquote 2026-09-12 · cryptoquote 2026-09-17 · cryptoquote 2026-09-19
### model-error (78)
celebrity-cipher 2025-01-11 · celebrity-cipher 2025-02-10 · celebrity-cipher 2025-03-27 · celebrity-cipher 2025-04-14 · celebrity-cipher 2025-04-29 · celebrity-cipher 2025-05-29 · celebrity-cipher 2025-06-19 · celebrity-cipher 2025-06-26 · celebrity-cipher 2025-07-01 · celebrity-cipher 2025-09-26 · celebrity-cipher 2025-09-27 · celebrity-cipher 2025-10-18 · celebrity-cipher 2025-11-14 · celebrity-cipher 2025-11-15 · celebrity-cipher 2025-11-17 · celebrity-cipher 2025-11-18 · celebrity-cipher 2025-12-09 · celebrity-cipher 2025-12-18 · celebrity-cipher 2025-12-19 · celebrity-cipher 2025-12-20 · celebrity-cipher 2025-12-25 · celebrity-cipher 2026-01-01 · celebrity-cipher 2026-01-02 · celebrity-cipher 2026-01-05 · celebrity-cipher 2026-01-12 · celebrity-cipher 2026-01-24 · celebrity-cipher 2026-02-16 · celebrity-cipher 2026-02-26 · celebrity-cipher 2026-02-28 · celebrity-cipher 2026-04-21 · celebrity-cipher 2026-04-28 · celebrity-cipher 2026-05-09 · celebrity-cipher 2026-05-18 · celebrity-cipher 2026-05-27 · celebrity-cipher 2026-06-02 · celebrity-cipher 2026-06-09 · celebrity-cipher 2026-07-20 · celebrity-cipher 2026-07-31 · celebrity-cipher 2026-08-04 · cryptoquip 2025-10-23 · cryptoquip 2025-10-31 · cryptoquip 2025-12-26 · cryptoquip 2026-01-30 · cryptoquip 2026-02-12 · cryptoquip 2026-02-19 · cryptoquip 2026-06-02 · cryptoquip 2026-06-11 · cryptoquip 2026-06-23 · cryptoquip 2026-09-16 · cryptoquote 2025-01-10 · cryptoquote 2025-01-21 · cryptoquote 2025-01-28 · cryptoquote 2025-02-15 · cryptoquote 2025-02-20 · cryptoquote 2025-03-27 · cryptoquote 2025-03-28 · cryptoquote 2025-07-10 · cryptoquote 2025-09-22 · cryptoquote 2025-10-02 · cryptoquote 2025-10-08 · cryptoquote 2025-11-13 · cryptoquote 2025-11-25 · cryptoquote 2025-12-11 · cryptoquote 2025-12-30 · cryptoquote 2026-01-08 · cryptoquote 2026-01-26 · cryptoquote 2026-02-23 · cryptoquote 2026-02-25 · cryptoquote 2026-04-15 · cryptoquote 2026-04-22 · cryptoquote 2026-05-01 · cryptoquote 2026-05-08 · cryptoquote 2026-07-08 · cryptoquote 2026-07-21 · cryptoquote 2026-08-25 · cryptoquote 2026-08-26 · cryptoquote 2026-09-04 · cryptoquote 2026-09-08
### tie (1)
cryptoquote 2026-08-08 — `C. DAY LEWIS` vs output `Z. DAY LEWIS`; single unconstrained initial, identical objective score. Not label noise (label is correct).

---

## 8. Files and reproducibility

- Analysis copy: `~/workspace/substitution-solver/rebuild_analysis/` (original `rebuild/`, `boxentriq-local/`, `www/` untouched).
- Code diff vs `rebuild/src`: `src/search.rs` (pin-scale fix only), `src/bin/bench.rs` (missing `name_w` build fix), new `src/bin/triage.rs` (diagnostic harness). All 22 unit tests pass (`--test-threads=2`).
- Raw triage rows: `triage_baseline.jsonl` (test, 48×12k pre-fix), `triage_test_fixed.jsonl` (test, 96×6k + pin fix), `triage_dev_*.jsonl` (dev runs). Note: rows for correct solves omit puzzle/answer/got text (space optimization); `ok_norm`/`verdict` are authoritative.
- Verdicts use the solver's true final objective including trigram (`full_score`), not the quad+word-only `score_key`.

## 9. Recommended next levers (not done here)

1. **Name model**: source a general public-figure name list (not test-derived) to extend the gazetteer; consider a name-part backoff so OOV names aren't scored at −27.8. Would address `PAZ VEGA`, `JAMES BALDWIN`, and several celebrity-cipher model errors.
2. **Denser word-bigram**: the 200k/50k tables leave literary bigrams (`REMAIN AJAR`, `BLOW THOU`) unseen; a larger table or character-backoff would sharpen close calls. Watch WASM size budget.
3. **Short-puzzle handling**: the 34–67-letter decile is 63%; consider a puzzle-length-aware schedule (more restarts when signal is thin).
4. **Rerank robustness**: the 2 dev regressions under 96×6,000 came from selection among more candidates; a more robust final selection (e.g. consensus across top candidates) could recover them.
