#!/usr/bin/env python3
"""fetch.py — download the day's raw puzzle assets.

Outputs a JSON manifest to stdout:
{
  "date": "2026-09-24",
  "cryptoquote_img": "assets/2026-09-24_cryptoquote.jpg | null",
  "cryptoquip_pdf":  "assets/2026-09-24_cryptoquip.pdf | null",
  "celeb_ciphers": [ {"date": "2026-09-24", "ciphertext": "...", "clue": "O=R",
                       "attribution": "AGATHA CHRISTIE"}, ... ]   # today + upcoming from week page
}

Sources:
- Cryptoquote: Arkansas Democrat-Gazette image, date-patterned
    https://cdn.wehco.com/adg/puzzles/{MMDD}/quote.jpg
- Cryptoquip: Cecil Daily section page -> latest article -> PDF link
    https://www.cecildaily.com/diversions/cryptoquip/
- Celebrity Cipher: cryptoquip.net week page (today + upcoming days)
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
        return save(f"{day:%Y-%m-%d}_cryptoquote.jpg", data), None
    except Exception as e:
        return None, f"{url}: {e}"


def fetch_cryptoquip(day):
    """Cecil Daily: section page -> latest /diversions/cryptoquip/ article -> PDF href."""
    try:
        html = get("https://www.cecildaily.com/diversions/cryptoquip/").decode("utf-8", "replace")
    except Exception as e:
        return None, f"section page: {e}"
    arts = sorted(set(
        m.group(1) for m in
        re.finditer(r'href="((?:https://www\.cecildaily\.com)?/diversions/cryptoquip/[^"]+)"', html)
        if m.group(1).rstrip("/") != "/diversions/cryptoquip"
    ))
    if not arts:
        return None, "no article links found on section page"
    # latest article first (BLOX lists newest first; try each until a PDF is found)
    for art in arts[:6]:
        url = art if art.startswith("http") else "https://www.cecildaily.com" + art
        try:
            ahtml = get(url).decode("utf-8", "replace")
        except Exception:
            continue
        pdfs = re.findall(r'href="([^"]+\.pdf[^"]*)"', ahtml, re.I)
        if pdfs:
            pdf = pdfs[0] if pdfs[0].startswith("http") else \
                ("https://www.cecildaily.com" + pdfs[0] if pdfs[0].startswith("/") else pdfs[0])
            try:
                data = get(pdf)
                return save(f"{day:%Y-%m-%d}_cryptoquip.pdf", data), None
            except Exception as e:
                return None, f"pdf download {pdf}: {e}"
    return None, f"no PDF in first {min(6, len(arts))} articles"


def fetch_celebrity_ciphers():
    """cryptoquip.net week page: per-date ciphertext + clue (+ attribution)."""
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
    return uniq, None


def main():
    day = datetime.now()
    if len(sys.argv) > 1:
        day = datetime.strptime(sys.argv[1], "%Y-%m-%d")
    manifest = {"date": day.strftime("%Y-%m-%d"), "errors": {}}

    img, err = fetch_cryptoquote(day)
    manifest["cryptoquote_img"] = img
    if err:
        manifest["errors"]["cryptoquote"] = err

    pdf, err = fetch_cryptoquip(day)
    manifest["cryptoquip_pdf"] = pdf
    if err:
        manifest["errors"]["cryptoquip"] = err

    ciphers, err = fetch_celebrity_ciphers()
    manifest["celeb_ciphers"] = ciphers
    if err:
        manifest["errors"]["celebrity_cipher"] = err

    print(json.dumps(manifest, indent=1))


if __name__ == "__main__":
    main()
