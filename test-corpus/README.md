# Substitution-Cipher Test Corpus

Exact puzzle+answer pairs scraped from **cryptoquip.net** for benchmarking a
substitution-cipher solver. **TEST DATA ONLY — never hardcode these answers
into solver code.**

## Files

- `corpus.json` — JSON array of 1671 entries, each
  `{type, date, puzzle, answer, source_url}`.
  - `type`: `cryptoquip` | `cryptoquote` | `celebrity-cipher`
  - `date`: puzzle date in `YYYY-MM-DD` (taken from the page's own date
    heading, e.g. "Cryptoquip 20 September 2026" or the per-day "Date:
    MM/DD/YYYY" line; falls back to the post slug date)
  - `puzzle`: ciphertext exactly as published (HTML entities decoded, so
    `&#8217;` becomes the literal `’` U+2019 the page renders)
  - `answer`: exact plaintext answer incl. punctuation and author attribution
  - `source_url`: the cryptoquip.net post the pair was taken from
- `qa_notes.json` — posts/blocks that could not be extracted and why.

## Source / endpoint used

WordPress REST API (no key required), category **"Old Puzzles"** (id 48):

- `GET https://cryptoquip.net/wp-json/wp/v2/posts?categories=48&per_page=100&page=N&_fields=slug,date,link,content`
- 25 pages × 100 posts = 2439 posts total in the category (fetched 2026-09-23).
- Target posts selected by slug prefix: `cryptoquip-answer*`,
  `cryptoquote-answer*`, `*celebrity-cipher*`. (Note: a browser-like
  `User-Agent` header is required; default python-urllib requests get 403.)
- Archive/category pages and RSS were not needed — the API covered everything.

## Counts

| type             | entries | date range              |
|------------------|---------|-------------------------|
| cryptoquip       | 619     | 2025-01-05 → 2026-09-20 |
| cryptoquote      | 531     | 2025-01-07 → 2026-09-21 |
| celebrity-cipher | 521     | 2024-12-30 → 2026-09-19 |
| **total**        | **1671**|                         |

Cryptoquip/Cryptoquote posts are one puzzle per day; Celebrity Cipher posts
are weekly (Mon–Sat, 6 puzzles per post) and were split into per-day entries.

## Extraction notes / data-quality caveats

- **Exactness verified** by cross-checking rendered pages
  (e.g. `/cryptoquip-answer-09-21-2026/`,
  `/cryptoquote-answer-09-21-2026/`,
  `/celebrity-cipher-answers-from-sep-14-to-19-2026/`) against the corpus —
  character-for-character match including curly quotes/apostrophes, `…`, `–`/`−`.
- **Duplicate dates kept as separate entries** (4 cases): the site published
  two different puzzles under the same date (e.g. reposts with `-2` slugs and
  one late-published duplicate). Each entry has its own `source_url`.
- **Site typos exist in the source data.** A substitution-consistency check
  (puzzle↔answer letter mapping must be bijective) flags 143 of 1671 entries
  as internally inconsistent on cryptoquip.net itself (e.g. a dropped word in
  the puzzle, a swapped letter, a stray character). Transcription is faithful
  to the published pages; filter these out if your benchmark requires
  perfectly consistent ciphers.
- **2 items unrecoverable:**
  - `celebrity-cipher-answer-from-sep-01-to-06-2025` — post contains only a
    PDF embed (scanned, no extractable text).
  - `2025-05-15` block of `celebrity-cipher-answer-from-may-12-to-17-2025` —
    the puzzle text is truncated on the site (`"AK Y VAS…`); excluded rather
    than guessed.
- **Corrected during parsing (documented, not hidden):** one weekly post
  (`...-dec-29-to-jan-03-2026`) mislabels its January dates as 2025; years
  were rolled forward so dates increase monotonically within the week. One
  weekly block had its year split across nested tags (`09/14/202</strong>6`);
  handled.

## Sample entries

cryptoquip / 2026-09-20 — https://cryptoquip.net/cryptoquip-answer-09-21-2026/
- puzzle: `XPJ XLCSUH XLYX VJEOH RJ CV XLWB’FW HWPCSU XLCSUH PLCEW XLWB’FW PYEOCSU: SWWREW YSR XFWYR.`
- answer: `TWO THINGS THAT FOLKS DO IF THEY’RE SEWING THINGS WHILE THEY’RE WALKING: NEEDLE AND TREAD.`

cryptoquote / 2026-09-21 — https://cryptoquip.net/cryptoquote-answer-09-21-2026/
- puzzle: `RG, LIKUIOCIP! DEA RPI UGI JEEPHRD UE UGI LIRLEM UGRU RHRTIML OD LEAZ. − KIFFD UEMID GEPUEM`
- answer: `AH, SEPTEMBER! YOU ARE THE DOORWAY TO THE SEASON THAT AWAKENS MY SOUL. − PEGGY TONEY HORTON`

celebrity-cipher / 2026-09-14 — https://cryptoquip.net/celebrity-cipher-answers-from-sep-14-to-19-2026/
- puzzle: `“FNMDHGN J’Z SWW PEJLOSNTNU, J’Z JTMDKDFXN WP KOWTJTL DTRSOJTL JT … J SDVN YODS J UW PWE D XJQJTL GNEJWHGXR.” – ZJMODNX VNDSWT`
- answer: `“BECAUSE I’M TOO FRIGHTENED, I’M INCAPABLE OF PHONING ANYTHING IN … I TAKE WHAT I DO FOR A LIVING SERIOUSLY.” – MICHAEL KEATON`
