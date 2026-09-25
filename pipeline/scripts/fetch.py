#!/usr/bin/env python3
"""fetch.py — download the day's raw puzzle assets.

Usage: fetch.py [YYYY-MM-DD] [--types=cryptoquote,cryptoquip,celebrity_cipher]

Outputs a JSON manifest to stdout:
{
  "date": "2026-09-24",
  "cryptoquote_img": "assets/2026-09-24_cryptoquote.jpg | null",
  "cryptoquip_pdf":  "assets/2026-09-24_cryptoquip.pdf | null",
  "celeb_ciphers": [ {"date": "2026-09-24", "ciphertext": "...", "clue": "O=R",
                      "attribution": "AGATHA CHRISTIE"} ]   # target day only (1/day)
}

Sources:
- Cryptoquote: Arkansas Democrat-Gazette image, date-patterned
    https://cdn.wehco.com/adg/puzzles/{MMDD}/quote.jpg
- Cryptoquip: Cecil Daily section page -> latest article -> PDF link
    https://www.cecildaily.com/diversions/cryptoquip/
- Celebrity Cipher: cryptoquip.net week page (target day only, 1/day)
    https://cryptoquip.net/todays-celebrity-cipher-answer/
"""
import json
import os
import re
import sys
import urllib.request
from datetime import datetime, timedelta
from html import unescape

UA = {"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
                    "(KHTML, like Gecko) Chrome/126.0 Safari/537.36"}

ASSETS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets")


def get(url, timeout=45):
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def save(name, data):
    os.makedirs(ASSETS, exist_ok=True)
    p = os.path.join(ASSETS, name)
    with open(p, "wb") as f:
        f.write(data)
    return p


def fetch_cryptoquote(day):
    mdd = day.strftime("%m%d")
    url = f"https://cdn.wehco.com/adg/puzzles/{mdd}/quote.jpg"
    try:
        data = get(url)
        if len(data) < 5000:
            return None, f"too small ({len(data)}b)"
        path = save(f"{day:%Y-%m-%d}_cryptoquote.jpg", data)
        ok, detail = verify_printed_date(path, day)
        if not ok:
            return None, f"date mismatch: {detail}"
        return path, None
    except Exception as e:
        return None, f"{url}: {e}"


def verify_printed_date(img_path, day):
    """Check the Arkansas cryptoquote image's printed corner date (e.g. '9-25')
    matches the requested day. The date-patterned CDN URL has served a stale
    (previous-day) image for a future date, so never trust the URL alone.
    Fail closed on a positive mismatch; proceed with a warning only when no
    date-like string is OCR'd at all (downstream review checks still apply)."""
    import re
    import subprocess
    try:
        r = subprocess.run(["tesseract", img_path, "stdout", "--psm", "6"],
                           capture_output=True, text=True, timeout=90)
        text = r.stdout or ""
    except Exception as e:
        return True, f"date check skipped (tesseract unavailable: {e})"
    want = f"{day.month}-{day.day}"
    seen = []
    for m in re.finditer(r"(\d{1,2})[-/](\d{1,2})", text):
        got = f"{int(m.group(1))}-{int(m.group(2))}"
        seen.append(m.group(0))
        if got == want:
            return True, f"printed date {m.group(0)} matches {want}"
    if seen:
        return False, f"printed date {seen[0]} != target {want}"
    return True, "no printed date OCR'd; proceeding (downstream checks apply)"


def article_published_date(ahtml):
    """Extract YYYY-MM-DD published date from article HTML (BLOX/TownNews meta)."""
    pats = [
        r'<meta[^>]+property="article:published_time"[^>]+content="([^"]+)"',
        r'<meta[^>]+content="([^"]+)"[^>]+property="article:published_time"',
        r'"datePublished"\s*:\s*"([^"]+)"',
        r'<time[^>]+datetime="([^"]+)"',
        r'<meta[^>]+name="publish-date"[^>]+content="([^"]+)"',
    ]
    for pat in pats:
        m = re.search(pat, ahtml, re.I)
        if m:
            dm = re.match(r"(\d{4})-(\d{2})-(\d{2})", m.group(1))
            if dm:
                return f"{dm.group(1)}-{dm.group(2)}-{dm.group(3)}"
    return None


def fetch_cryptoquip(day):
    """Cecil Daily: section page -> article published ON `day` -> PDF href.

    Never mislabel: only accept an article whose published date matches the
    target day. If the day's article isn't out yet, fail with an error instead
    of silently attaching a different day's PDF.
    Returns (pdf_path, error, info_dict)."""
    want = day.strftime("%Y-%m-%d")
    try:
        html = get("https://www.cecildaily.com/diversions/cryptoquip/").decode("utf-8", "replace")
    except Exception as e:
        return None, f"section page: {e}", {}
    arts = sorted(set(
        m.group(1) for m in
        re.finditer(r'href="((?:https://www\.cecildaily\.com)?/diversions/cryptoquip/[^"]+)"', html)
        if m.group(1).rstrip("/") != "/diversions/cryptoquip"
    ))
    if not arts:
        return None, "no article links found on section page", {}
    seen = []
    # newest first (BLOX lists newest first); take the first article dated `want`
    for art in arts[:6]:
        url = art if art.startswith("http") else "https://www.cecildaily.com" + art
        try:
            ahtml = get(url).decode("utf-8", "replace")
        except Exception:
            continue
        pub = article_published_date(ahtml)
        if pub and pub not in seen:
            seen.append(pub)
        if pub != want:
            continue
        pdfs = re.findall(r'href="([^"]+\.pdf[^"]*)"', ahtml, re.I)
        if pdfs:
            pdf = pdfs[0] if pdfs[0].startswith("http") else \
                ("https://www.cecildaily.com" + pdfs[0] if pdfs[0].startswith("/") else pdfs[0])
            try:
                data = get(pdf)
                return save(f"{day:%Y-%m-%d}_cryptoquip.pdf", data), None, \
                    {"article": url, "article_date": pub}
            except Exception as e:
                return None, f"pdf download {pdf}: {e}", {"article": url, "article_date": pub}
    if seen:
        return None, f"no cryptoquip article published for {want} (seen: {sorted(seen)[:4]})", {}
    return None, f"no PDF in first {min(6, len(arts))} articles (no parseable dates)", {}


def fetch_celebrity_ciphers(day):
    """cryptoquip.net week page: per-date ciphertext + clue (+ attribution).

    Returns ONLY the cipher for the target day (1/day) — the caller, not the
    parser, decides which date is wanted.
    """
    try:
        html = get("https://cryptoquip.net/todays-celebrity-cipher-answer/").decode("utf-8", "replace")
    except Exception as e:
        return [], f"week page: {e}"
    text = re.sub(r"<script[\s\S]*?</script>", " ", html)
    text = re.sub(r"<style[\s\S]*?</style>", " ", text)
    text = re.sub(r"<[^>]+>", "\n", text)
    text = unescape(text)  # &#8220; -> " etc., else ciphertext keeps raw entities
    text = re.sub(r"[ \t]+", " ", text)
    lines = [l.strip() for l in text.split("\n") if l.strip()]
    out = []
    # Blocks look like: "Date: 09/24/2026 (Thursday)" ... "ciphertext" ... "− ATTRIBUTION"
    # ... "Clue: O=R" ... "Celebrity Cipher Answer Date: 09/24/2026"
    cur = None
    for i, l in enumerate(lines):
        dm = re.match(r"Date:\s*(\d{2})/(\d{2})/(\d{4})", l)
        if dm and "answer date" not in l.lower():
            if cur and cur.get("ciphertext"):
                out.append(cur)
            cur = {"date": f"{dm.group(3)}-{dm.group(1)}-{dm.group(2)}",
                   "ciphertext": "", "clue": "", "attribution": ""}
            continue
        if cur is None:
            continue
        if not cur["ciphertext"]:
            # ciphertext line: long, mostly uppercase incl. quotes/dashes
            s = l.strip('"“” ')
            letters = sum(c.isalpha() for c in s)
            if len(s) > 40 and letters > 20 and sum(c.isupper() for c in s if c.isalpha()) / max(1, letters) > 0.85:
                # split trailing " − ATTRIBUTION"
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
    # de-dupe by date, keep first
    seen, uniq = set(), []
    for c in out:
        if c["date"] not in seen:
            seen.add(c["date"])
            uniq.append(c)
    # 1/day: keep only the target date's cipher
    want = day.strftime("%Y-%m-%d")
    day_only = [c for c in uniq if c["date"] == want]
    if not day_only:
        return [], f"no celebrity cipher for {want} on week page (saw: {[c['date'] for c in uniq][:5]})"
    return day_only, None


def main():
    day = datetime.now()
    types = {"cryptoquote", "cryptoquip", "celebrity_cipher"}
    args = []
    for a in sys.argv[1:]:
        if a.startswith("--types="):
            types = set(a.split("=", 1)[1].split(","))
        else:
            args.append(a)
    if args:
        day = datetime.strptime(args[0], "%Y-%m-%d")
    manifest = {"date": day.strftime("%Y-%m-%d"), "errors": {}, "types": sorted(types)}

    if "cryptoquote" in types:
        img, err = fetch_cryptoquote(day)
        manifest["cryptoquote_img"] = img
        if err:
            manifest["errors"]["cryptoquote"] = err

    if "cryptoquip" in types:
        pdf, err, info = fetch_cryptoquip(day)
        manifest["cryptoquip_pdf"] = pdf
        manifest["cryptoquip_article"] = info.get("article")
        manifest["cryptoquip_article_date"] = info.get("article_date")
        if err:
            manifest["errors"]["cryptoquip"] = err

    if "celebrity_cipher" in types:
        ciphers, err = fetch_celebrity_ciphers(day)
        manifest["celeb_ciphers"] = ciphers
        if err:
            manifest["errors"]["celebrity_cipher"] = err

    print(json.dumps(manifest, indent=1))


if __name__ == "__main__":
    main()
