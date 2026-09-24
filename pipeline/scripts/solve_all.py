#!/usr/bin/env python3
"""solve_all.py — solve each puzzle with the Rust solver, run review checks,
write results/<date>.json with teaser + answer (never the full ciphertext).

Usage: solve_all.py <puzzles.json> [--solver /path/to/solve_one]

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


def teaser(ciphertext, n=48):
    t = re.sub(r"\s+", " ", ciphertext).strip()
    return (t[:n] + "…") if len(t) > n else t


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
        entry = {"type": p["type"], "date": p["date"], "source": p["source"],
                 "attribution": SOURCE_ATTR.get(p["type"], p["source"]),
                 "teaser": teaser(p["ciphertext"]), "clue": p.get("clue", ""),
                 "ocr_conf": p.get("ocr_conf")}
        ct = p["ciphertext"]
        if not ct or len(re.sub(r"[^A-Za-z]", "", ct)) < 20:
            entry["status"] = "no_ciphertext"
            entry["needs_review"] = True
            results.append(entry)
            continue
        try:
            sol = run_solver(solver, ct, p.get("clue", ""))
        except Exception as e:
            entry["status"] = f"solver_error: {e}"
            entry["needs_review"] = True
            results.append(entry)
            continue
        checks = {
            "clue_ok": check_clue(p.get("clue", ""), sol.get("key", "")),
            "reencode_ok": check_reencode(ct, sol.get("plaintext", ""), sol.get("key", "")),
            "common_words_ok": check_common_words(sol.get("plaintext", "")),
        }
        entry.update({
            "answer": sol["plaintext"],
            "solver": {"key": sol.get("key"), "score": sol.get("score"),
                       "ms": sol.get("ms"), "restarts": 192, "steps": 6000},
            "checks": checks,
            "needs_review": not all(checks.values()),
            "status": "ok" if all(checks.values()) else "flagged",
        })
        # never republish the full ciphertext: keep only the teaser
        results.append(entry)

    out = {"date": day,
           "generated_at": datetime.now(timezone.utc).isoformat(),
           "puzzles": results}
    os.makedirs(os.path.join(BASE, "results"), exist_ok=True)
    op = os.path.join(BASE, "results", f"{day}.json")
    with open(op, "w") as f:
        json.dump(out, f, indent=1)
    n_ok = sum(1 for r in results if r.get("status") == "ok")
    print(f"wrote {op}: {n_ok}/{len(results)} ok")
    print(json.dumps(out, indent=1)[:3000])


if __name__ == "__main__":
    main()
