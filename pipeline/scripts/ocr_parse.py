#!/usr/bin/env python3
"""ocr_parse.py — OCR puzzle assets and extract ciphertext + clue + date.

Usage: ocr_parse.py <manifest.json>
Reads the manifest from fetch.py, runs tesseract (via pdftoppm for PDFs),
and prints a puzzles JSON:
{
  "date": "2026-09-24",
  "puzzles": [
    {"type": "cryptoquote", "date": "2026-09-24", "ciphertext": "...", "clue": "",
     "source": "arkansasonline", "asset": "assets/...jpg", "ocr_conf": 87.3},
    {"type": "cryptoquip", ...},
    {"type": "celebrity_cipher", "date": "2026-09-25", "ciphertext": "...",
     "clue": "S=D", "source": "cryptoquip.net", ...}
  ]
}
"""
import json
import os
import re
import subprocess
import sys
import tempfile

BASE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")


def ocr_image(path):
    """Run tesseract on an image file; return (text, mean_conf)."""
    with tempfile.NamedTemporaryFile(suffix=".txt", delete=False) as tf:
        out_base = tf.name[:-4]
    try:
        subprocess.run(["tesseract", path, out_base, "--psm", "6", "-l", "eng"],
                       capture_output=True, timeout=300)
        with open(out_base + ".txt", encoding="utf-8", errors="replace") as f:
            text = f.read()
        # mean confidence via TSV
        conf = None
        try:
            r = subprocess.run(["tesseract", path, "stdout", "--psm", "6", "-l", "eng", "tsv"],
                               capture_output=True, timeout=300, text=True)
            vals = [int(l.split("\t")[10]) for l in r.stdout.splitlines()[1:]
                    if len(l.split("\t")) > 10 and l.split("\t")[10].lstrip("-").isdigit()
                    and int(l.split("\t")[10]) >= 0]
            if vals:
                conf = round(sum(vals) / len(vals), 1)
        except Exception:
            pass
        return text, conf
    finally:
        for e in (".txt",):
            try:
                os.unlink(out_base + e)
            except OSError:
                pass


def ocr_pdf(path):
    """Render PDF pages with pdftoppm, OCR each, return (text, mean_conf)."""
    tmp = tempfile.mkdtemp(prefix="cqpdf_")
    try:
        subprocess.run(["pdftoppm", "-png", "-r", "200", path, os.path.join(tmp, "p")],
                       capture_output=True, timeout=300, check=True)
        texts, confs = [], []
        for fn in sorted(os.listdir(tmp)):
            if fn.endswith(".png"):
                t, c = ocr_image(os.path.join(tmp, fn))
                texts.append(t)
                if c is not None:
                    confs.append(c)
        conf = round(sum(confs) / len(confs), 1) if confs else None
        return "\n".join(texts), conf
    finally:
        for fn in os.listdir(tmp):
            os.unlink(os.path.join(tmp, fn))
        os.rmdir(tmp)


def norm_ws(s):
    return re.sub(r"\s+", " ", s).strip()


def extract_clue(text):
    m = re.search(r"(?:clue|today'?s clue)\s*[:\-]?\s*([A-Z])\s*=\s*([A-Z])", text, re.I)
    if m:
        return f"{m.group(1).upper()}={m.group(2).upper()}"
    m = re.search(r"\b([A-Z])\s*=\s*([A-Z])\b", text)
    return f"{m.group(1)}={m.group(2)}" if m else ""


def cipher_runs(text, min_len=30):
    """Long mostly-uppercase runs = ciphertext candidates."""
    cands = []
    for m in re.finditer(r"[A-Z][A-Z'’‘.,;:\-?!()\"“”/ ]{25,}", text):
        s = norm_ws(m.group(0)).strip(" .,;:-")
        letters = sum(c.isalpha() for c in s)
        if len(s) >= min_len and letters >= 15 and \
           sum(c.isupper() for c in s if c.isalpha()) / letters > 0.85:
            cands.append(s)
    # longest first, de-dupe substrings
    cands.sort(key=len, reverse=True)
    out = []
    for c in cands:
        if not any(c in o or o in c for o in out):
            out.append(c)
    return out


def despace_cipher(block):
    """Newspaper puzzles often print wide gaps between letters; tesseract may
    return single capitals separated by spaces, with 2+ spaces/newlines at real
    word breaks. Reconstruct words."""
    words = re.split(r"(?: {2,}|\n|\r)+", block)
    out = []
    for w in words:
        toks = w.split()
        if len(toks) > 1 and all(len(t) == 1 and t.isupper() for t in toks):
            out.append("".join(toks))
            continue
        # stray detached edge letter: "A VFPACFGAHC" -> "AVFPACFGAHC"
        if len(toks) > 2 and len(toks[0]) == 1 and toks[0].isupper():
            toks = ["".join(toks[:2])] + toks[2:]
        if len(toks) > 2 and len(toks[-1]) == 1 and toks[-1].isupper():
            toks = toks[:-2] + ["".join(toks[-2:])]
        if w.strip():
            out.append(" ".join(toks) if toks else "")
    return " ".join(o for o in out if o)


def cipher_block_after(head, marker_re):
    """Text between a header marker and the trailing em-dash attribution."""
    m = re.search(marker_re, head, re.I)
    seg = head[m.end():] if m else head
    # trailing attribution like "— FVR ENDIFWM"
    am = re.search(r"[—–-]\s*([A-Z][A-Z .'\-]{2,30})\s*$", seg.strip())
    attr = am.group(1).strip() if am else ""
    if am:
        seg = seg[: am.start()]
    return seg, attr


def parse_cryptoquote_ocr(text):
    """Ciphertext block sits between the CRYPTOQUOTE header and the encrypted
    author attribution; Yesterday's answer tail is kept for verification."""
    head = re.split(r"yesterday'?s cryptoquote", text, flags=re.I)[0]
    seg, author_cipher = cipher_block_after(head, r"cryptoquote")
    cipher = despace_cipher(seg)
    # sanity: keep only plausible cipher text (mostly uppercase)
    cipher = re.sub(r"[^A-Z'’.,;:\-?!()\"“”/ ]", " ", cipher)
    cipher = norm_ws(cipher)
    # puzzle date printed like "9-24"
    pdate = None
    dm = re.search(r"\b(\d{1,2})-(\d{1,2})\b", head[:500])
    if dm:
        pdate = (int(dm.group(1)), int(dm.group(2)))
    yesterday = None
    m = re.search(r"yesterday'?s cryptoquote\s*:\s*(.+?)(?:—|--|–)\s*([A-Z][A-Z .'\-]+)\s*$",
                  text, re.I | re.S)
    if m:
        yesterday = {"answer": norm_ws(m.group(1)), "author": norm_ws(m.group(2))}
    if author_cipher:
        cipher = (cipher + " — " + author_cipher).strip()
    return cipher, yesterday, pdate


def parse_cryptoquip_ocr(text):
    seg, _ = cipher_block_after(text, r"cryptoquip")
    # drop the clue line from the cipher text
    seg = re.sub(r"(?:today'?s\s+)?clue\s*[:\-]?\s*[A-Z]\s*=\s*[A-Z]", " ", seg, flags=re.I)
    cipher = despace_cipher(seg)
    cipher = re.sub(r"[^A-Z'’.,;:\-?!()\"“”/ ]", " ", cipher)
    return norm_ws(cipher)


def main():
    with open(sys.argv[1]) as f:
        man = json.load(f)
    day = man["date"]
    puzzles = []

    img = man.get("cryptoquote_img")
    if img and os.path.exists(os.path.join(BASE, img) if not os.path.isabs(img) else img):
        p = img if os.path.isabs(img) else os.path.join(BASE, img)
        text, conf = ocr_image(p)
        cipher, yesterday, pdate = parse_cryptoquote_ocr(text)
        puzzles.append({"type": "cryptoquote", "date": day, "ciphertext": cipher,
                        "clue": "", "source": "arkansasonline",
                        "asset": img, "ocr_conf": conf,
                        "printed_date": pdate,
                        "yesterday_answer": yesterday,
                        "ocr_text": text[:2000]})

    pdf = man.get("cryptoquip_pdf")
    if pdf and os.path.exists(pdf if os.path.isabs(pdf) else os.path.join(BASE, pdf)):
        p = pdf if os.path.isabs(pdf) else os.path.join(BASE, pdf)
        text, conf = ocr_pdf(p)
        cipher = parse_cryptoquip_ocr(text)
        puzzles.append({"type": "cryptoquip", "date": day, "ciphertext": cipher,
                        "clue": extract_clue(text), "source": "cecildaily",
                        "asset": pdf, "ocr_conf": conf,
                        "ocr_text": text[:2000]})

    for c in man.get("celeb_ciphers", []):
        puzzles.append({"type": "celebrity_cipher", "date": c["date"],
                        "ciphertext": c["ciphertext"], "clue": c.get("clue", ""),
                        "attribution": c.get("attribution", ""),
                        "source": "cryptoquip.net", "asset": None,
                        "ocr_conf": None})

    print(json.dumps({"date": day, "puzzles": puzzles}, indent=1))


if __name__ == "__main__":
    main()
