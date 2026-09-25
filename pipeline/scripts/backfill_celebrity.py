#!/usr/bin/env python3
"""Backfill celebrity-cipher archive from cryptoquip.net weekly pages into D1.

Usage: backfill_celebrity.py <weeks.txt> [--limit N] [--solver PATH]

For each weekly archive URL: fetch the page, parse per-date cipher blocks
(ciphertext + clue + date ONLY -- the published answers on those pages are
never extracted or stored), solve each with the project solver, apply the
same review checks as the daily pipeline, and insert clean answers into D1.
Refused/failed entries are logged, never stored. Idempotent: dates already
in D1 are skipped.

Answers are derived by our solver, never lifted from the site.
"""
import json
import os
import re
import subprocess
import sys
import urllib.request
from html import unescape

BASE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, "/home/hatch/workspace/substitution-solver/cloudflare")
from cf import api  # noqa: E402

AID = "8aee88d9ea2ea8e660a82a12ce8fd47f"
D1 = "40a46205-2aa4-4d26-8bd4-dd8614ac0225"
UA = {"User-Agent": "Mozilla/5.0 (X11; Linux x86_64)"}

# review-check word sets (same as solve_all.py)
COMMON = {"THE", "AND", "OF", "TO", "A", "IN", "IS", "IT", "YOU", "THAT",
          "HE", "WAS", "FOR", "ON", "ARE", "AS", "WITH", "HIS", "THEY", "I",
          "AT", "BE", "THIS", "HAVE", "FROM"}
DICT = set()
for _dp in (os.path.join(BASE, "..", "data", "words.txt"),
            "/usr/share/dict/words"):
    if os.path.exists(_dp):
        with open(_dp) as _f:
            DICT = {w.strip().upper() for w in _f if w.strip()}
        break


def get(url):
    req = urllib.request.Request(url, headers=UA)
    return urllib.request.urlopen(req, timeout=60).read().decode("utf-8", "replace")


def parse_week(html):
    """Extract per-date cipher blocks. Stops before any answer section."""
    text = re.sub(r"<script[\s\S]*?</script>", " ", html)
    text = re.sub(r"<style[\s\S]*?</style>", " ", text)
    text = re.sub(r"<[^>]+>", "\n", text)
    text = unescape(text)
    text = re.sub(r"[ \t]+", " ", text)
    lines = [l.strip() for l in text.split("\n") if l.strip()]
    out = []
    cur = None
    in_answer = False
    for l in lines:
        if "celebrity cipher answer date" in l.lower():
            # published answer follows -- skip until the next Date: block.
            # (answers are never extracted or stored)
            in_answer = True
            continue
        dm = re.match(r"Date:\s*(\d{2})/(\d{2})/(\d{4})", l)
        if dm:
            in_answer = False
            if cur and cur.get("ciphertext"):
                out.append(cur)
            cur = {"date": f"{dm.group(3)}-{dm.group(1)}-{dm.group(2)}",
                   "ciphertext": "", "clue": "", "attribution": ""}
            continue
        if in_answer or cur is None:
            continue
        if not cur["ciphertext"]:
            s = l.strip('"“” ')
            letters = sum(c.isalpha() for c in s)
            if len(s) > 40 and letters > 20 and \
               sum(c.isupper() for c in s if c.isalpha()) / max(1, letters) > 0.85:
                parts = re.split(r"\s+[−–-]\s+(?=[A-Z][A-Z .'\-()]+$)", s)
                cur["ciphertext"] = parts[0].strip().strip('"“”')
                if len(parts) > 1:
                    cur["attribution"] = parts[1].strip()
                continue
        cm = re.match(r"Clue:\s*([A-Z])\s*=\s*([A-Z])", l, re.I)
        if cm and not cur["clue"]:
            cur["clue"] = f"{cm.group(1).upper()}={cm.group(2).upper()}"
    if cur and cur.get("ciphertext"):
        out.append(cur)
    seen, uniq = set(), []
    for c in out:
        if c["date"] not in seen:
            seen.add(c["date"])
            uniq.append(c)
    return uniq


def check_clue(clue, key):
    if not clue or "=" not in clue or not key or len(key) != 26:
        return True
    c, p = clue.split("=")
    return key[ord(c) - 65] == p


def check_reencode(ciphertext, plaintext, key):
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
        else:
            out.append(ch if ch != " " else " ")
    ct = re.sub(r"[^A-Z]", "", ciphertext.upper())
    pt = re.sub(r"[^A-Z]", "", "".join(out).upper())
    return ct == pt


def check_common_words(plaintext):
    words = set(re.findall(r"[A-Za-z]+", plaintext.upper()))
    return len(words & COMMON) >= 2


def check_word_ratio(plaintext, min_ratio=0.65):
    if not DICT:
        return True
    body = re.split(r"\s+—\s*|\s+--\s*|\s+-\s+(?=[A-Z][A-Z ]+$)", plaintext)[0]
    words = [w for w in re.findall(r"[A-Za-z]+", body.upper()) if len(w) >= 3]
    if not words:
        return False
    return sum(1 for w in words if w in DICT) / len(words) >= min_ratio


def d1(sql, params=None):
    r = api("POST", f"/accounts/{AID}/d1/database/{D1}/query",
            {"sql": sql, "params": params or []})
    return r[0] if isinstance(r, list) else r


def main():
    weeks = [l.strip() for l in open(sys.argv[1]) if l.strip()]
    limit = int(sys.argv[sys.argv.index("--limit") + 1]) if "--limit" in sys.argv else 0
    solver = (sys.argv[sys.argv.index("--solver") + 1] if "--solver" in sys.argv
              else "/home/hatch/workspace/substitution-solver/rebuild/target/release/solve_one")
    if limit:
        weeks = weeks[:limit]

    have = {r["day"] for r in
            d1("SELECT day FROM answers WHERE type='celebrity_cipher'")["results"]}
    print(f"already in D1: {len(have)} celebrity-cipher days", flush=True)

    kept = refused = failed = 0
    log = open("/tmp/backfill_celebrity.log", "a")
    for wi, url in enumerate(weeks):
        try:
            blocks = parse_week(get(url))
        except Exception as e:
            print(f"[{wi}] fetch fail {url}: {e}", flush=True)
            continue
        for b in blocks:
            day = b["date"]
            if day in have:
                continue
            try:
                r = subprocess.run(
                    [solver, "--puzzle", b["ciphertext"], "--clue", b["clue"] or "",
                     "--restarts", "192", "--steps", "6000"],
                    capture_output=True, text=True, timeout=600)
                sol = json.loads(r.stdout)
                pt, key = sol["plaintext"], sol["key"]
                checks = {"clue_ok": check_clue(b["clue"], key),
                          "reencode_ok": check_reencode(b["ciphertext"], pt, key),
                          "common_words_ok": check_common_words(pt),
                          "word_ratio_ok": check_word_ratio(pt)}
                if not all(checks.values()):
                    refused += 1
                    log.write(f"REFUSED {day} {checks}\n"); log.flush()
                    continue
                teaser = " ".join(b["ciphertext"].split()[:8])
                res = d1(
                    """INSERT INTO answers (day,type,source,teaser,clue,answer,attribution,
                        solver_key,solver_score,solver_ms,ocr_conf,checks_json,needs_review,status,generated_at)
                       VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                       ON CONFLICT(day,type) DO UPDATE SET answer=excluded.answer,
                        solver_key=excluded.solver_key,solver_score=excluded.solver_score,
                        solver_ms=excluded.solver_ms,checks_json=excluded.checks_json,
                        ingested_at=datetime('now')""",
                    [day, "celebrity_cipher", "cryptoquip.net", teaser, b["clue"] or None,
                     pt, b.get("attribution") or "Celebrity Cipher by Luis Campos, via cryptoquip.net",
                     key, sol.get("score"), sol.get("ms"), None,
                     json.dumps(checks), 0, "ok", "backfill"])
                if res.get("success"):
                    kept += 1; have.add(day)
                else:
                    failed += 1
                    log.write(f"D1FAIL {day} {str(res)[:150]}\n"); log.flush()
            except Exception as e:
                failed += 1
                log.write(f"ERROR {day} {str(e)[:150]}\n"); log.flush()
        print(f"[{wi}/{len(weeks)}] {url[-30:]} kept={kept} refused={refused} failed={failed}",
              flush=True)
    print(f"DONE kept={kept} refused={refused} failed={failed}")


main()
