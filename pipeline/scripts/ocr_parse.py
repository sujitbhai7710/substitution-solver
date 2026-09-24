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
    """Run tesseract on an image file; return (text, mean_conf).

    Newspaper thumbnails (e.g. 304px wide) OCR badly; upscale small images
    first so tesseract sees ~1200px-wide text.
    """
    use_path, tmp_big = path, None
    try:
        from PIL import Image
        im = Image.open(path)
        w, h = im.size
        if w < 900:
            s = 1200.0 / w
            im = im.resize((int(w * s), int(h * s)), Image.LANCZOS)
            tmp_big = path + ".big.png"
            im.save(tmp_big)
            use_path = tmp_big
    except Exception:
        pass
    try:
        return _tesseract(use_path)
    finally:
        if tmp_big:
            try:
                os.unlink(tmp_big)
            except OSError:
                pass


def _tesseract(path):
    with tempfile.NamedTemporaryFile(suffix=".txt", delete=False) as tf:
        out_base = tf.name[:-4]
    try:
        subprocess.run(["tesseract", path, out_base, "--psm", "6", "-l", "eng"],
                       capture_output=True, timeout=300)
        with open(out_base + ".txt", encoding="utf-8", errors="replace") as f:
            text = f.read()
        # mean confidence via TSV
        conf = None
        tsv_debug = {}
        try:
            r = subprocess.run(["tesseract", path, "stdout", "--psm", "6", "-l", "eng", "tsv"],
                               capture_output=True, timeout=300, text=True)
            tsv_debug["rc"] = r.returncode
            tsv_debug["stdout_len"] = len(r.stdout or "")
            tsv_debug["stderr"] = (r.stderr or "")[:200]
            lines = (r.stdout or "").splitlines()
            tsv_debug["n_lines"] = len(lines)
            tsv_debug["sample"] = [ln[:100] for ln in lines[1:4]]
            vals = [int(l.split("\t")[10]) for l in lines[1:]
                    if len(l.split("\t")) > 10 and l.split("\t")[10].lstrip("-").isdigit()
                    and int(l.split("\t")[10]) >= 0]
            tsv_debug["n_words"] = len(vals)
            if vals:
                conf = round(sum(vals) / len(vals), 1)
        except Exception as e:
            tsv_debug["exc"] = f"{type(e).__name__}: {e}"[:200]
        return text, conf, tsv_debug
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
        texts, confs, debugs = [], [], []
        for fn in sorted(os.listdir(tmp)):
            if fn.endswith(".png"):
                t, c, dbg = ocr_image(os.path.join(tmp, fn))
                texts.append(t)
                debugs.append(dbg)
                if c is not None:
                    confs.append(c)
        conf = round(sum(confs) / len(confs), 1) if confs else None
        return "\n".join(texts), conf, debugs
    finally:
        for fn in os.listdir(tmp):
            os.unlink(os.path.join(tmp, fn))
        os.rmdir(tmp)


def norm_ws(s):
    return re.sub(r"\s+", " ", s).strip()


def extract_clue(text):
    # Ignore the how-to-play instructions ("X equals O" example): the clue
    # always sits before them. (Yesterday's block is kept: on the Cecil sheet
    # the clue line comes AFTER yesterday's answer.)
    head = re.split(r"the\s+cryptoquip\s+is\s+a\s+substitution",
                    text, flags=re.I)[0]
    m = re.search(r"clue\s*[:\-]?\s*([A-Z])\s*(?:=|equals)\s*([A-Z])", head, re.I)
    if m:
        return f"{m.group(1).upper()}={m.group(2).upper()}"
    m = re.search(r"\b([A-Z])\s*=\s*([A-Z])\b", head)
    return f"{m.group(1)}={m.group(2)}" if m else ""


def cipher_runs(text, min_len=30, keep_ws=False):
    """Long mostly-uppercase runs = ciphertext candidates, in document order.
    keep_ws preserves newlines/multiple spaces (needed by despace_cipher)."""
    cands = []
    for m in re.finditer(r"[A-Z][A-Z'’‘.,;:\-?!()\"“”/ ]{25,}", text):
        s = m.group(0) if keep_ws else norm_ws(m.group(0))
        s = s.strip(" .,;:-")
        letters = sum(c.isalpha() for c in s)
        if len(s) >= min_len and letters >= 15 and \
           sum(c.isupper() for c in s if c.isalpha()) / letters > 0.85:
            cands.append((m.start(), s))
    # de-dupe substrings (longest first), then restore document order —
    # word order matters for solving, so never sort by length.
    cands.sort(key=lambda c: len(c[1]), reverse=True)
    out = []
    for _, s in cands:
        if not any(s in o or o in s for _, o in out):
            out.append((_, s))
    out.sort(key=lambda c: c[0])
    return [s for _, s in out]


def despace_cipher(block):
    """Newspaper puzzles often print wide gaps between letters; tesseract may
    return single capitals separated by spaces, with 2+ spaces/newlines at real
    word breaks. Reconstruct words."""
    words = re.split(r"(?: {2,}|\n|\r)+", block)
    out = []
    for w in words:
        if not w.strip():
            continue
        # set trailing/leading punctuation aside: "Q F F," -> core "Q F F"
        pm = re.match(r"^(.*?)([.,;:!?\"'’‘()—–-]+)$", w.strip())
        core, punct = (pm.group(1), pm.group(2)) if pm else (w, "")
        toks = core.split()
        if len(toks) > 1 and all(len(t) == 1 and t.isupper() for t in toks):
            out.append("".join(toks) + punct)
            continue
        # stray detached edge letter: "A VFPACFGAHC" -> "AVFPACFGAHC"
        if len(toks) > 2 and len(toks[0]) == 1 and toks[0].isupper():
            toks = ["".join(toks[:2])] + toks[2:]
        if len(toks) > 2 and len(toks[-1]) == 1 and toks[-1].isupper():
            toks = toks[:-2] + ["".join(toks[-2:])]
        out.append((" ".join(toks) if toks else "") + punct)
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
    author attribution; Yesterday's answer tail is kept for verification.

    Junk guards: instructions/sample sit above the header; if the header is
    OCR-mangled we still pick only long uppercase runs, so mixed-case
    instruction text can never leak into the cipher.
    """
    head = re.split(r"yesterday['’ʼ]?s cryptoquote", text, flags=re.I)[0]
    if re.search(r"cryptoquote", head, re.I):
        seg, author_cipher = cipher_block_after(head, r"cryptoquote")
        cipher = despace_cipher(seg)
    else:
        # header OCR-mangled: fall back to long uppercase runs only, so the
        # mixed-case instructions/sample ("AXYDLBAAXR is LONGFELLOW") can
        # never leak into the cipher. Spacing is preserved for despace.
        seg = "\n".join(cipher_runs(head, keep_ws=True))
        author_cipher = ""
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
    m = re.search(r"yesterday['’ʼ]?s cryptoquote\s*:?\s*(.+?)(?:—|--|–)\s*~?\s*([A-Z][A-Z .'’~\-]+?)\s*$",
                  text, re.I | re.S)
    if m:
        yesterday = {"answer": norm_ws(m.group(1)), "author": norm_ws(m.group(2))}
    if author_cipher:
        author_cipher = norm_ws(despace_cipher(
            re.sub(r"[^A-Z'’.,;:\-?!()\"“”/ ]", " ", author_cipher)))
        cipher = (cipher + " — " + author_cipher).strip()
    return cipher, yesterday, pdate


def parse_cryptoquip_ocr(text):
    """Ciphertext sits right under the CRYPTOQUIP header. Everything after the
    Yesterday's-answer / clue / how-to-play markers is junk (the yesterday
    answer is already-solved PLAINTEXT and must never enter the cipher)."""
    seg, _ = cipher_block_after(text, r"cryptoquip")
    seg = re.split(
        r"yesterday['’ʼ]?s\s+cryptoquip"
        r"|today['’ʼ]?s\s+cryptoquip\s+clue"
        r"|the\s+cryptoquip\s+is\s+a\s+substitution",
        seg, flags=re.I)[0]
    # drop any residual clue line, both "Z=I" and "Z equals I" wordings
    seg = re.sub(r"(?:today['’ʼ]?s\s+)?clue\s*[:\-]?\s*[A-Z]\s*(?:=|equals)\s*[A-Z]",
                 " ", seg, flags=re.I)
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
        text, conf, dbg = ocr_image(p)
        cipher, yesterday, pdate = parse_cryptoquote_ocr(text)
        puzzles.append({"type": "cryptoquote", "date": day, "ciphertext": cipher,
                        "clue": "", "source": "arkansasonline",
                        "asset": img, "ocr_conf": conf,
                        "printed_date": pdate,
                        "yesterday_answer": yesterday,
                        "ocr_text": text[:2000],
                        "ocr_debug": dbg})

    pdf = man.get("cryptoquip_pdf")
    if pdf and os.path.exists(pdf if os.path.isabs(pdf) else os.path.join(BASE, pdf)):
        p = pdf if os.path.isabs(pdf) else os.path.join(BASE, pdf)
        text, conf, dbgs = ocr_pdf(p)
        cipher = parse_cryptoquip_ocr(text)
        puzzles.append({"type": "cryptoquip", "date": day, "ciphertext": cipher,
                        "clue": extract_clue(text), "source": "cecildaily",
                        "asset": pdf, "ocr_conf": conf,
                        "ocr_text": text[:2000],
                        "ocr_debug": dbgs})

    for c in man.get("celeb_ciphers", []):
        puzzles.append({"type": "celebrity_cipher", "date": c["date"],
                        "ciphertext": c["ciphertext"], "clue": c.get("clue", ""),
                        "attribution": c.get("attribution", ""),
                        "source": "cryptoquip.net", "asset": None,
                        "ocr_conf": None})

    print(json.dumps({"date": day, "puzzles": puzzles,
                       "fetch_errors": man.get("errors", {})}, indent=1))


if __name__ == "__main__":
    main()
