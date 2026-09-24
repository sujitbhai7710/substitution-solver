#!/usr/bin/env python3
"""Score boxentriq black-box benchmark results against the test corpus.

Usage: python3 score.py [--results boxentriq_results.jsonl]

- Filters to substitution-consistent entries: the puzzle<->answer letter
  alignment must be bijective (each cipher letter maps to exactly one plain
  letter and vice versa; non-letters are ignored for the mapping but must
  agree in letter-ness positionally).
- EXACT-match accuracy: letters compared case-insensitively, all other
  characters (punctuation, Unicode) compared byte-exact.
- Prints per-type + overall accuracy and median/mean solve ms.
- Writes boxentriq_misses.txt (type + date of every wrong entry).
"""
import argparse
import json
import os
import re
import statistics
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
LETTER = re.compile(r"[A-Za-zÀ-ɏ]")  # ASCII + Latin extended letters


def is_letter(ch):
    return bool(LETTER.fullmatch(ch))


def is_consistent(puzzle, answer):
    if len(puzzle) != len(answer):
        return False
    c2p, p2c = {}, {}
    for pc, ac in zip(puzzle, answer):
        pl, al = is_letter(pc), is_letter(ac)
        if pl != al:
            return False
        if not pl:
            continue
        pu, au = pc.upper(), ac.upper()
        if c2p.get(pu, au) != au:
            return False
        if p2c.get(au, pu) != pu:
            return False
        c2p[pu] = au
        p2c[au] = pu
    return True


def exact_match(a, b):
    if a is None or b is None or len(a) != len(b):
        return False
    for ca, cb in zip(a, b):
        la, lb = is_letter(ca), is_letter(cb)
        if la and lb:
            if ca.upper() != cb.upper():
                return False
        elif ca != cb:
            return False
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--results", default=os.path.join(HERE, "boxentriq_results.jsonl"))
    args = ap.parse_args()

    rows = []
    with open(args.results, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))

    per_type = {}
    misses = []
    n_inconsistent = 0
    n_errors = 0
    for r in rows:
        t = per_type.setdefault(
            r["type"], {"total": 0, "consistent": 0, "scored": 0, "correct": 0, "ms": []}
        )
        t["total"] += 1
        if not is_consistent(r["puzzle"], r["answer"]):
            n_inconsistent += 1
            continue
        t["consistent"] += 1
        if r.get("box_plaintext") is None:
            n_errors += 1
            continue
        t["scored"] += 1
        t["ms"].append(r["box_ms"])
        if exact_match(r["box_plaintext"], r["answer"]):
            t["correct"] += 1
        else:
            misses.append(f"{r['type']}\t{r['date']}")

    print("=== boxentriq black-box benchmark ===")
    print(f"results file: {args.results}")
    print(
        f"entries: {len(rows)} | inconsistent (excluded): {n_inconsistent} "
        f"| solver errors (excluded): {n_errors}"
    )
    print()
    print(f"{'type':<18}{'total':>6}{'consist':>9}{'scored':>8}{'acc%':>8}{'median_ms':>11}{'mean_ms':>9}")
    g_scored = g_correct = 0
    g_ms = []
    for typ in sorted(per_type):
        t = per_type[typ]
        acc = 100.0 * t["correct"] / t["scored"] if t["scored"] else float("nan")
        med = statistics.median(t["ms"]) if t["ms"] else float("nan")
        mean = statistics.fmean(t["ms"]) if t["ms"] else float("nan")
        g_scored += t["scored"]
        g_correct += t["correct"]
        g_ms += t["ms"]
        print(
            f"{typ:<18}{t['total']:>6}{t['consistent']:>9}{t['scored']:>8}"
            f"{acc:>8.2f}{med:>11.0f}{mean:>9.0f}"
        )
    g_acc = 100.0 * g_correct / g_scored if g_scored else float("nan")
    g_med = statistics.median(g_ms) if g_ms else float("nan")
    g_mean = statistics.fmean(g_ms) if g_ms else float("nan")
    print(
        f"{'OVERALL':<18}{len(rows):>6}{'':>9}{g_scored:>8}"
        f"{g_acc:>8.2f}{g_med:>11.0f}{g_mean:>9.0f}"
    )
    print()
    print(f"misses: {len(misses)}")
    miss_path = os.path.join(HERE, "boxentriq_misses.txt")
    with open(miss_path, "w", encoding="utf-8") as f:
        for m in misses:
            f.write(m + "\n")
    print(f"wrote {miss_path}")


if __name__ == "__main__":
    sys.exit(main())
