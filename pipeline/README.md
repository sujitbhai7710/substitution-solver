# Daily puzzle pipeline

Automated daily answers for Cryptoquip, Cryptoquote, and Celebrity Cipher.

## Architecture

```
GitHub Action (scheduled, .github/workflows/daily.yml)
  1. fetch.py      download raw assets
  2. ocr_parse.py  OCR (tesseract) + extract ciphertext/clue/date
  3. solve_all.py  solve with Rust solver + review checks
  4. commit        results/YYYY-MM-DD.json -> repo
        |
        v
Cloudflare Worker `puzzle-answers` (scheduled daily, after the Action)
  pulls results JSON from raw.githubusercontent.com -> D1 `answers` table
  serves GET /answers?date=YYYY-MM-DD and /answers/latest
```

No secrets needed: the results repo is public, the Worker reads raw files.

## Sources (user-approved)

| Puzzle | Source | Asset |
|---|---|---|
| Cryptoquip | cecildaily.com/diversions/cryptoquip/ | daily PDF (section page -> latest article -> PDF link) |
| Cryptoquote | arkansasonline.com/puzzles/quote/ | `https://cdn.wehco.com/adg/puzzles/{MMDD}/quote.jpg` (date-patterned) |
| Celebrity Cipher | cryptoquip.net/todays-celebrity-cipher-answer/ | week page: today + upcoming days (ciphertext + clue as text, no OCR) |

## Scripts

- `scripts/fetch.py [YYYY-MM-DD]` — downloads assets, prints manifest JSON.
  Celebrity-cipher fetch parses the cryptoquip.net week page into per-date
  `{date, ciphertext, clue, attribution}` entries.
- `scripts/ocr_parse.py <manifest.json>` — tesseract OCR (images directly,
  PDFs via `pdftoppm`), extracts ciphertext/clue, handles newspaper
  spaced-letter layouts, splits off "Yesterday's Cryptoquote" as verification
  data. Prints puzzles JSON.
- `scripts/solve_all.py <puzzles.json> --solver <solve_one>` — solves each
  puzzle at 192 restarts x 6000 steps, runs review checks:
  - `clue_ok`: given clue mapping holds in the solved key
  - `reencode_ok`: re-encoding the plaintext reproduces the ciphertext
  - `common_words_ok`: solution contains common English function words
  Any failure -> `needs_review: true` (published but flagged).
  Writes `results/YYYY-MM-DD.json` with **teaser + answer only**
  (never the full publisher ciphertext).

## D1 schema (database `puzzle-answers`)

```sql
CREATE TABLE answers (
  day TEXT, type TEXT, source TEXT, teaser TEXT, clue TEXT, answer TEXT,
  attribution TEXT, solver_key TEXT, solver_score REAL, solver_ms REAL,
  ocr_conf REAL, checks_json TEXT, needs_review INTEGER, status TEXT,
  generated_at TEXT, ingested_at TEXT DEFAULT (datetime('now')),
  PRIMARY KEY (day, type)
);
CREATE TABLE pipeline_runs (
  day TEXT PRIMARY KEY, started_at TEXT, finished_at TEXT,
  status TEXT, log TEXT
);
```

## Future: LLM review pass

`solve_all.py` review checks are heuristic. For a real LLM correction pass,
add a repo secret (e.g. `OPENAI_API_KEY`) and a step that sends low-confidence
solves for review. Not wired up — no AI credentials provided yet.
