#!/usr/bin/env python3
"""solve_all.py — solve each puzzle with the Rust solver, run review checks,
write results/<date>.json with full ciphertext + teaser + answer.

Usage: solve_all.py <puzzles.json> [--solver /path/to/solve_one]

For OCR'd puzzles, puzzles.json may carry cipher_variants (one parse per
tesseract PSM). Each variant is solved; the first whose decode passes all
review checks wins. Fallback: the variant with the most checks passing.

Results files MERGE: a run covering only some types replaces those types'
entries and keeps the others, so three per-type scheduled runs share one
daily file without clobbering each other.

Review checks (no external LLM needed):
  - clue_ok:        the given clue mapping holds in the solved key
  - reencode_ok:    re-encoding the plaintext with the inverse key reproduces
                    the ciphertext exactly (catches replay/codec bugs)
  - common_words_ok: solution contains common English function words
Any failure -> needs_review = true (published but flagged).
"""
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timezone

BASE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
COMMON = {"THE", "AND", "OF", "TO", "A", "IN", "IS", "IT", "YOU", "THAT",
          "HE", "WAS", "FOR", "ON", "ARE", "AS", "WITH", "HIS", "THEY", "I"}

DICT = set()
_dict_path = os.path.join(BASE, "data", "words.txt")
if os.path.exists(_dict_path):
    with open(_dict_path) as f:
        DICT = {w.strip().upper() for w in f if w.strip()}

SOURCE_ATTR = {
    "cryptoquote": "Cryptoquote © King Features Syndicate, via Arkansas Democrat-Gazette",
    "cryptoquip": "Cryptoquip © King Features Syndicate, via Cecil Daily",
    "celebrity_cipher": "Celebrity Cipher by Luis Campos, via cryptoquip.net",
}


def run_solver(solver, ciphertext, clue, restarts=192, steps=6000):
    r = subprocess.run(
        [solver, "--puzzle", ciphertext, "--clue", clue,
         "--restarts", str(restarts), "--steps", str(steps)],
        capture_output=True, text=True, timeout=600)
    if r.returncode != 0:
        raise RuntimeError(f"solver failed: {r.stderr[:300]}")
    return json.loads(r.stdout)


def check_clue(clue, key):
    """clue like 'O=R' means cipher O -> plain R. key is 26-char plain for A..Z."""
    if not clue or "=" not in clue or not key or len(key) != 26:
        return True
    c, p = clue.split("=")
    return key[ord(c) - 65] == p


def check_reencode(ciphertext, plaintext, key):
    """Encode plaintext letters back through inverse key; must equal ciphertext letters."""
    inv = {}
    for i, p in enumerate(key):
        inv[p] = chr(65 + i)
    out = []
    for ch in plaintext:
        if ch.isalpha():
            c = inv.get(ch.upper())
            if c is None:
                return False
            out.append(c)
        elif ch.isalpha() is False and ch not in " ":
            out.append(ch)
        else:
            out.append(" ")
    # compare letter sequences only (punctuation/whitespace preserved by replay)
    ct_letters = re.sub(r"[^A-Z]", "", ciphertext.upper())
    pt_letters = re.sub(r"[^A-Z]", "", "".join(out).upper())
    return ct_letters == pt_letters


def check_common_words(plaintext):
    words = set(re.findall(r"[A-Za-z]+", plaintext.upper()))
    return len(words & COMMON) >= 2


def check_word_ratio(plaintext, min_ratio=0.65):
    """Most solved body words (len>=3) must be real English words.

    Catches garbage-in/garbage-out: bad OCR decodes to non-words even when
    clue/reencode/common-words checks pass. Attribution (after -- or em dash)
    is excluded since names aren't in the dictionary.
    """
    if not DICT:
        return True
    body = re.split(r"\s+—\s*|\s+--\s*|\s+-\s+(?=[A-Z][A-Z ]+$)", plaintext)[0]
    words = [w for w in re.findall(r"[A-Za-z]+", body.upper()) if len(w) >= 3]
    if not words:
        return False
    hits = sum(1 for w in words if w in DICT)
    return (hits / len(words)) >= min_ratio


def check_ocr_conf(ocr_conf, min_conf=50.0):
    """Tesseract mean word confidence must exist and clear a floor."""
    return ocr_conf is not None and ocr_conf >= min_conf


def teaser(ciphertext, n=48):
    t = re.sub(r"\s+", " ", ciphertext).strip()
    return (t[:n] + "…") if len(t) > n else t


def solve_puzzle(solver, p):
    """Solve one puzzle, trying each OCR variant; return (entry, winning_ct).

    The first variant whose decode passes every review check wins; otherwise
    the variant with the most checks passing is kept (flagged).
    """
    variants = p.get("cipher_variants") or [p.get("ciphertext", "")]
    # de-dupe, drop empties, keep order
    seen, cts = set(), []
    for ct in variants:
        ct = (ct or "").strip()
        if ct and ct not in seen and len(re.sub(r"[^A-Za-z]", "", ct)) >= 20:
            seen.add(ct)
            cts.append(ct)
    entry = {"type": p["type"], "date": p["date"], "source": p["source"],
             "attribution": p.get("attribution") or SOURCE_ATTR.get(p["type"], p["source"]),
             "clue": p.get("clue", ""), "ocr_conf": p.get("ocr_conf")}
    if not cts:
        entry.update({"status": "no_ciphertext", "needs_review": True,
                      "ciphertext": "", "teaser": ""})
        return entry, ""
    best = None  # (n_checks_ok, all_ok, ct, sol, checks)
    for ct in cts:
        try:
            sol = run_solver(solver, ct, p.get("clue", ""))
        except Exception as e:
            cand = (0, False, ct, None,
                    {"solver_error": str(e)[:120]})
            if best is None or cand[0] > best[0]:
                best = cand
            continue
        checks = {
            "clue_ok": check_clue(p.get("clue", ""), sol.get("key", "")),
            "reencode_ok": check_reencode(ct, sol.get("plaintext", ""), sol.get("key", "")),
            "common_words_ok": check_common_words(sol.get("plaintext", "")),
            "word_ratio_ok": check_word_ratio(sol.get("plaintext", "")),
            # puzzles with no OCR step (text sources) pass ocr_conf vacuously
            "ocr_conf_ok": check_ocr_conf(p.get("ocr_conf")) if p.get("asset") else True,
        }
        n_ok = sum(1 for v in checks.values() if v)
        cand = (n_ok, all(checks.values()), ct, sol, checks)
        if best is None or (cand[1] and not best[1]) or \
           (cand[1] == best[1] and cand[0] > best[0]):
            best = cand
        if cand[1]:
            break  # first fully-clean variant wins
    _, all_ok, ct, sol, checks = best
    entry["ciphertext"] = ct
    entry["teaser"] = teaser(ct)
    if sol is None:
        entry.update({"status": checks.get("solver_error", "solver_error"),
                      "needs_review": True, "checks": {}})
        return entry, ct
    entry.update({
        "answer": sol["plaintext"],
        "solver": {"key": sol.get("key"), "score": sol.get("score"),
                   "ms": sol.get("ms"), "restarts": 192, "steps": 6000},
        "checks": checks,
        "needs_review": not all_ok,
        "status": "ok" if all_ok else "flagged",
    })
    return entry, ct


TYPE_ORDER = ["cryptoquote", "cryptoquip", "celebrity_cipher"]


def main():
    puzzles_path = sys.argv[1]
    solver = None
    for i, a in enumerate(sys.argv):
        if a == "--solver":
            solver = sys.argv[i + 1]
    if not solver:
        cand = os.path.join(BASE, "..", "rebuild", "target", "release", "solve_one")
        solver = cand
    with open(puzzles_path) as f:
        data = json.load(f)
    day = data["date"]
    results = []
    for p in data["puzzles"]:
        entry, _ = solve_puzzle(solver, p)
        results.append(entry)

    op = os.path.join(BASE, "results", f"{day}.json")
    # MERGE with any existing entries (per-type scheduled runs share the file)
    merged = {}
    if os.path.exists(op):
        try:
            prev = json.load(open(op))
            for r in prev.get("puzzles", []):
                merged[r["type"]] = r
        except Exception:
            pass
    for r in results:
        merged[r["type"]] = r
    puzzles = [merged[t] for t in TYPE_ORDER if t in merged] + \
              [r for t, r in merged.items() if t not in TYPE_ORDER]

    out = {"date": day,
           "generated_at": datetime.now(timezone.utc).isoformat(),
           "puzzles": puzzles,
           "diagnostics": {
               "fetch_errors": data.get("fetch_errors", {}),
               "puzzle_types": [r["type"] for r in puzzles],
               "ocr_conf": {r["type"]: r.get("ocr_conf") for r in puzzles},
           }}
    os.makedirs(os.path.join(BASE, "results"), exist_ok=True)
    with open(op, "w") as f:
        json.dump(out, f, indent=1)
    n_ok = sum(1 for r in puzzles if r.get("status") == "ok")
    print(f"wrote {op}: {n_ok}/{len(puzzles)} ok (this run: {len(results)} types)")
    print(json.dumps(out, indent=1)[:3000])


if __name__ == "__main__":
    main()
