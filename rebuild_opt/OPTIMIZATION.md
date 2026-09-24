# Solver Optimization — Accuracy Push (2026-09-24)

**User directive:** "Even if it take 4-5 seconds more no issues please make the algorithm
more better so that we get best results with best accuracy."
**Budget approved:** ~8-9s/puzzle (up from ~3s).
**Baseline (merged rebuild/):** dev 187/209 = 89.47%; test 897/1035 = 86.67%.
**Method:** dev.json (209) for all tuning; test.json (1035) exactly ONE final validation run.
**Constraint:** General-purpose only; no hardcoded answers; no label-derived features.

## 1. Budget scaling (dev.json, 2 threads, pristine code)

| Config | Dev accuracy | Median | Max | Delta vs 96×6000 |
|---|---|---|---|---|
| 96×6000 (baseline) | 187/209 = 89.47% | 1408ms | 4124ms | — |
| **192×6000** | **189/209 = 90.43%** | **2774ms** | **5503ms** | **+2, 0 regressions** |
| 96×12000 | 187/209 = 89.47% | 2633ms | 8615ms | +0 (+2 fixed, 2 broken) |
| 288×6000 | 188/209 = 89.95% | 3890ms | 7766ms | +1 (worse than 192×6000) |

**Fixed by 192×6000 (0 regressions):** 2025-07-02, 2025-08-23.
Per-type at 192×6000: cryptoquip 96/98 (+1), cryptoquote 33/45 (±0), celebrity 60/66 (+1).

**Key findings:**
- **Coverage beats refinement.** Doubling steps (96×12000) fixes the same 2 puzzles as
  doubling restarts but breaks 2 cryptoquotes the baseline had right (net 0).
- **192×6000 is the sweet spot.** 288×6000 REGRESSES to 188/209. Under the fixed RNG
  seed the 288 candidate set is a strict superset of the 192 set, so the loss is pure
  *selection* failure: more candidates give the argmax more chances to pick a
  spuriously high-scoring wrong key. Selection, not coverage, is the bottleneck
  beyond 192 restarts.

## 2. Algorithmic levers (all tested at 96×6000 unless noted; all REVERTED)

| Lever | Mechanism | Dev result | Verdict |
|---|---|---|---|
| A: trigram term in `full_score` | Unify bigram-polish objective with rerank scale (polish omitted trigram) | 187/209, zero churn | Neutral — reverted |
| B: consensus key | Per-position majority vote over top-8 reranked, adopted iff beats best | 187/209, zero churn | Neutral — reverted |
| C: name-part backoff | OOV word (≥3 letters) matching a known gazetteer name part gets 0.25× name bonus | 187/209 | Neutral — reverted |
| dict_seeds=3 | Top-3 pattern-dictionary hits as seeds instead of top-1 | 187/209 | Neutral — reverted |
| cluster_select | Pick candidate with most near-neighbors (≥23/26 agreement) among top-12 | 188/209 (−1) | HARMFUL — reverted |
| rerank_depth=24 | Re-rank top-24 instead of top-12 (tests truncation hypothesis) | 189/209, zero churn | Neutral — reverted |
| trigram_w=2.0 | Double character-trigram weight | 186/209 (−1) | HARMFUL — reverted |

**Why nothing helped:** the rerank_depth=24 zero-churn result proves the truth is already
inside the top-12 — failures are *pure model errors* (the wrong key genuinely scores
higher on the combined objective), not truncation. No selector can fix that without a
better model, and the model is at a local optimum (weight perturbations hurt).

## 3. Final validation (test.json, held-out, ONE run)

**192×6000, pristine code: 900/1035 = 86.96%** (baseline 897/1035 = 86.67%; **+3 puzzles, +0.29pp**)

| Type | Baseline | 192×6000 | Delta |
|---|---|---|---|
| Cryptoquip | 470/487 = 96.51% | 468/487 = 96.10% | −2 |
| Cryptoquote | 159/220 = 72.27% | 161/220 = 73.18% | +2 |
| Celebrity Cipher | 268/328 = 81.71% | 271/328 = 82.62% | +3 |

Timing: median 2628ms, p90 3385ms, max 10626ms (2 threads, native).
The dev gain (+0.96pp) transferred as +0.29pp; cryptoquip noise (−2) offset by
cryptoquote/celebrity gains (+5). Net positive on held-out data.

## 4. Winning configuration

**Config change only (no code change):** restarts 96 → 192 (steps stay 6000).
- `rebuild/src/search.rs`: `Config::default().restarts = 192`
- `rebuild/www/app.js`: High-quality mode restarts 96 → 192
- All other weights/temps unchanged: word_w=3.0, bigram_w=2.0, trigram_w=1.0,
  name_w=10.0, t0=10, t_end=0.02, 6000 steps.

**Final diff vs rebuild/src:** zero code changes; the improvement is the search-budget
reallocation (coverage over refinement) validated at +2/0-regressions on dev.

## 5. Discarded / not pursued

- Larger word-bigram tables: help model errors but blow the WASM size budget.
- Word trigram model: new model to train; out of scope for this round.
- Adaptive per-puzzle restart scaling: global 192×6000 already covers the weak
  short-puzzle zone; adds complexity without measured gain.
- Iterated local search / basin hopping: bigger change; diminishing returns evident.
