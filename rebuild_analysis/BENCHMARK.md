# Benchmark Methodology

## Corpus

The benchmark uses a corpus of 1,671 puzzles collected from cryptoquip.net:
- Cryptoquip: 619 puzzles
- Cryptoquote: 531 puzzles  
- Celebrity Cipher: 521 puzzles

Of these, 1,244 are "exact-winnable": substitution-consistent by letters AND
with matching puzzle/answer punctuation (required for exact-match replay).

## Metric: Exact Match

A solve counts as correct only if the produced plaintext matches the answer
**exactly**:
- Letters compare case-insensitively (A-Z only participate in substitution)
- Punctuation, Unicode typography (curly quotes, em dashes), whitespace,
  repeated spaces, tabs, and line breaks must match exactly
- The solver preserves these via canonicalize → solve symbols → replay literals

This is stricter than letters-only accuracy. Boxentriq's solver strips or
normalizes punctuation, so its exact-match score is much lower than its
letters-only score.

## Boxentriq Comparison

Head-to-head comparison on a deterministic stratified manifest of 120
exact-winnable puzzles (40 per type). Boxentriq solved with its standard
settings (low-memory fourgram mode, 100 rounds, 10,000 steps, 5 retained
results).

Results reported as:
- Exact match % (both solvers)
- Letters-only % (both solvers)  
- Per type and overall
- Median solve times
- Representative failure cases

## Head-to-Head vs Boxentriq (120-puzzle stratified manifest, 40/type)

| Solver | Cryptoquip | Cryptoquote | Celebrity | Overall | Median |
|--------|-----------|-------------|-----------|---------|--------|
| Ours (exact) | 95.0% | 65.0% | 70.0% | **76.7%** | **0.70s** |
| Boxentriq (exact) | 0% | 0% | 0% | 0% | — |
| Boxentriq (letters-only) | 70.0% | 30.0% | 45.0% | 48.3% | 21–90s |

Boxentriq strips/normalizes punctuation, so its exact-match score is 0% by
construction. On letters-only it reaches 48.3% overall vs our 76.7% exact
(which implies ≥76.7% letters-only). We are also ~30–130× faster per puzzle
(0.7s vs 21–90s median).

Representative Boxentriq failures: MOVIE→MORIE, KIND→MIND, BOW→LOW,
Representative Boxentriq failures: MOVIE→MORIE, KIND→MIND, BOW→LOW,
RAY ROMANO→DAY DOMANO.

## WASM Size

Target: ≤1.26MB (Boxentriq's size). Achieved: **1.1MB**.

Size reduction strategy:
- Wordlist: 333k words (3.1MB) → 50k words (438KB) for WASM.
  Accuracy impact: negligible (77.5% vs 76.7% on h2h set, actually slightly better).
- Word bigrams: 1.8MB table omitted for WASM (bigram_w=0).
  Accuracy impact: -0.8% (75.8% vs 76.7%).
- Kept: quadgrams (447KB), trigrams (20KB).

The `#[cfg(target_arch = "wasm32")]` gates in `words.rs`, `bigrams.rs`, and
`lib.rs` select the small configuration for WASM builds only; native builds
use full data for benchmarking.

## Reproducing

```bash
# Native benchmark (all 1,244 exact-winnable)
cargo build --profile dist --features parallel --bin bench
./target/dist/bench --corpus ../test-corpus/corpus.json --threads 8 \
  --out results.jsonl --dump-misses misses.jsonl
```

## Current Results (2026-09-23, with dictionary attack)

| Type             | n   | Accuracy | Median |
|------------------|-----|----------|--------|
| Cryptoquip       | 585 | 95.21%   | 647ms  |
| Cryptoquote      | 265 | 71.70%   | 673ms  |
| Celebrity Cipher | 394 | 79.19%   | 839ms  |
| **Overall**      |1244 | **85.13%** | 699ms |

Baseline (round 1, old dict seed): 84.24% overall, 705ms median.
Pre-fix baseline (post-fivegram, broken dict attack): 84.57% overall.
The fixed dictionary attack (Olson-style MRV backtracking with top-2000
candidates) improved Cryptoquote from 68.68% to 71.70%.

## Round 3 Results (2026-09-23)

### Baseline (no correction)
Full consistent set (1,528 puzzles):
- Exact: 1,061/1,528 = 69.44%
- By type: Cryptoquip 558/595 (93.78%), Cryptoquote 191/513 (37.23%), Celebrity 312/420 (74.29%)
- Median: 2,951ms, Mean: 3,293ms

Exact-winnable subset (1,245 puzzles):
- Exact: 1,061/1,245 = 85.22%
- By type: Cryptoquip 558/585 (95.38%), Cryptoquote 191/266 (71.80%), Celebrity 312/394 (79.19%)
- Median: 3,007ms

### With Dictionary/Gazetteer Correction (strict thresholds)
Full consistent set:
- Exact: 1,059/1,528 = 69.31% (-2 vs baseline)
- By type: Cryptoquip 557/595 (93.61%), Cryptoquote 189/513 (36.84%), Celebrity 313/420 (74.52%)
- Median: 3,688ms (+737ms vs baseline), Mean: 4,090ms

Exact-winnable subset:
- Exact: 1,059/1,245 = 85.06% (-0.16pp vs baseline)
- By type: Cryptoquip 557/585 (95.21%), Cryptoquote 189/266 (71.05%), Celebrity 313/394 (79.44%)

Delta: Fixed 1 (celebrity-cipher 2026-06-02: JENDAYA→ZENDAYA via gazetteer),
       Broke 3 (cryptoquip 2026-01-21, cryptoquote 2026-05-27, cryptoquote 2026-08-21).

### Conclusion
The dictionary/gazetteer correction is implemented (src/correction.rs, src/gazetteer.rs)
but DISABLED by default (cfg.use_correction = false). Benchmarks show net-negative
impact: -2 accuracy, +737ms median latency. The scoring model (quad+word+trigram)
is not reliable enough to safely auto-correct; it sometimes prefers wrong
alternatives even with strict improvement thresholds (5.0 for dict, 0.5 for gazetteer).

The JENDAYA→ZENDAYA case demonstrates the potential value for proper-noun OOV,
but the risk of breaking correct solutions outweighs the benefit.

### WASM Binary
- Size: 1.2MB (under 1.26MB Boxentriq limit)
- Target: web (www/pkg/) and nodejs verified
- API: solve_js(puzzle, clue, restarts, steps) -> JSON {plaintext, key, score, ms}
- Note: Instant::now() not available in WASM; ms reported as 0. Frontend measures wall time.

### Frontend
- New www/ passes textarea value byte-for-byte unchanged to WASM.
- Old rawText() bug (normalized curly quotes/dashes to ASCII) is fixed.
- Preservation tests: www/preservation-tests.html, src/codec.rs tests.

---

## Round 4 (2026-09-23): Speed Regression Fix + Re-tune

### Speed Regression Root Cause

Round 3 showed median 2,951ms (vs 700ms in round 2). Investigation found TWO causes:

1. **Benchmark methodology artifact (main cause):** The benchmark ran with `--threads 8` on a 2-vCPU machine. Per-puzzle timings were inflated by oversubscription/contention (8 puzzles competing for 2 cores). Single-threaded measurement shows true per-solve latency is ~4-6x lower.

2. **Dictionary attack worst-case (tail latency):** The rewritten MRV dictionary attack could consume its full 200k-node budget on pathological puzzles (e.g., 18.8s on celebrity 2026-01-26 with zero hits). Added early-abort: if no complete hit found within 10,000 nodes, stop. (Measured: first-hit p50=19 nodes, p90=48, max=2,997 on sample.)

### Fixes Applied

- **Config defaults:** restarts 24→12, steps 12,000→6,000, dict_nodes 200,000→20,000, dict_cand_cap 2,000→500.
- **Dict early-abort:** stop at 10k nodes if no hit found (patdict.rs).
- **WASM panic fix:** `Instant::now()` panics on wasm32. Profiling timers now use `profile_now()` helper which returns None on WASM. (Found via Node sanity test.)
- **Frontend:** app.js now requests R=12/S=6000 (was R=24/S=12000).

### Restart × Steps Sweep (sample120, single-threaded)

| R | S | Acc | Median |
|---|---|-----|--------|
| 12 | 6,000 | 88.16% | 149ms |
| 12 | 12,000 | 86.84% | 230ms |
| 12 | 24,000 | 88.16% | 393ms |
| 24 | 6,000 | 88.16% | 299ms |
| 24 | 12,000 | 88.16% | 430ms |
| 48 | 24,000 | 88.16% | 1530ms |

Accuracy is FLAT across 12-48 restarts and 6k-24k steps (per-puzzle identical). The knee is **R=12, S=6,000**: same accuracy at 149ms vs 430ms (2.9x faster than old default).

### Word Bigram Experiment (Mandate Item 2)

**Result: Word bigrams buy 0.0 points. DROPPED from WASM plans.**

- Ablation on sample120: `--bigram-w 0` → 88.16% (identical per-puzzle to bigram-on).
- Ablation with BOTH bigram+trigram disabled → 88.16% (identical).
- The 1.8MB word bigram table does not change ANY outcome on the sample.
- Diagnostic (src/bin/diag.rs) shows misses are MODEL errors (quadgram confidently wrong, e.g., prefers THING over THINK by 5.5 points), not search errors. The bigram is too sparse/weak to override confident quadgram errors.
- **Decision:** Do NOT add pruned bigrams to WASM. The size budget is better spent elsewhere. Documented here per mandate.

### Full Corpus Results (Round 4 Final)

Single-threaded (`--threads 1`), new defaults. Compare to round 3 baseline.

**Full consistent (n=1,528):**
| Type | Round 4 | Round 3 | Δ |
|------|---------|---------|---|
| Cryptoquip | 558/595 = 93.78% | 557/595 = 93.61% | +1 |
| Cryptoquote | 190/513 = 37.04% | 190/513 = 37.04% | 0 |
| Celebrity | 312/420 = 74.29% | 312/420 = 74.29% | 0 |
| **Overall** | **1,060/1,528 = 69.37%** | 1,059/1,528 = 69.31% | **+1** |
| Median | **457ms** | 2,792ms | **6.1x faster** |
| Mean | 506ms | 3,173ms | 6.3x faster |

**Exact-winnable (n=1,244):**
| Type | Round 4 | Round 3 | Δ |
|------|---------|---------|---|
| Cryptoquip | 558/585 = 95.38% | 557/585 = 95.21% | +1 |
| Cryptoquote | 190/266 = 71.43% | 190/266 = 71.43% | 0 |
| Celebrity | 312/394 = 79.19% | 312/394 = 79.19% | 0 |
| **Overall** | **1,060/1,244 = 85.21%** | 1,059/1,245 = 85.06% | **+1** |
| Median | **458ms** | 2,820ms | **6.1x faster** |

**Per-puzzle delta:** Fixed 1 (cryptoquip 2026-01-21), Broke 0. Strictly better.

### WASM (Round 4)

- Size: **1,179,198 bytes** (1.12MB) — under 1.26MB budget.
- Node sanity tests: ALL PASS (curly apostrophe, clue lock, whitespace/tabs/newlines, em/en dashes, casing, real solve).
- Frontend updated to R=12/S=6000 defaults.

### Accuracy vs Target

- **Target:** ≥98% exact-match. **Achieved:** 85.21% winnable.
- **Gap:** 12.8 points. All sampled misses are MODEL errors (quadgram confidently prefers wrong answer). The LIKE/LIFE error class (e.g., THING vs THINK) is a quadgram blind spot, not fixable by more search (96 restarts doesn't help) or by the current word bigram (0 points).
- **Speed target:** 1-2s. **Achieved:** 0.46s median. ✅
- **Size target:** ≤1.26MB. **Achieved:** 1.12MB. ✅
- **Boxentriq:** We beat it head-to-head (76.7% exact vs 48.3% letters-only on h2h set, 30-130x faster). ✅

### Errors Fixed (Round 4)

1. `cargo` not on PATH → exported `$HOME/.cargo/bin`.
2. `bench.rs` missing `use_correction` field → added.
3. WASM panic on `Instant::now()` → `profile_now()` helper (cfg-gated for wasm32).
4. Test script used wrong JSON field (`solution` vs `plaintext`) → fixed.
5. Test puzzle too short (model limitation, not bug) → used longer puzzle.

## Round 5 (2026-09-23) — dev-set results (209 puzzles, all exact-winnable)

New stack: char 6-gram + word trigram LMs (models.bin 29MB), interpolated
score = chi*char + (1-chi)*word, SA with retuned temperatures.

| config | exact | median |
|---|---|---|
| planned (chi=0.5, R8/S6000, t0=20/0.6) | 27.27% (57/209) | 1.90s |
| chi=0.25, R16/S8000, t0=20/0.6 | 66.99% (140/209) | 5.04s |
| chi=0.25, R16/S8000, t0=10/0.3 | **80.38% (168/209)** | 5.41s |
| round-4 baseline, same dev set (R12/S6000) | 78.95% (165/209) | 0.25s |

Per-type at best config: cryptoquip 79.6% (round4: 89.8%), cryptoquote 62.2%
(round4: 55.6%), celebrity-cipher 93.9% (round4: 78.8%).

Key findings:
- The planned settings were far too hot: the new model's score deltas are much
  larger than the quadgram's, so t0=20 wandered randomly. t0=10/t_end=0.3 fixed it.
- Word model wants majority weight: chi=0.25 beat 0.5 (62%), 0.1 (68%), 0.0 (46%).
- Spot checks confirm the model ranks the true key highest; remaining misses are
  search errors, not model errors.
- Deployment blocker: models.bin is 29MB; it cannot ship inside the 1.12MB WASM
  budget. Round 5 needs a separate model download or server-side solving.
