#!/usr/bin/env python3
"""Build fivegrams.bin: top ~500k English character 5-grams with Lidstone-smoothed
natural-log probabilities, quantized to u8.

Corpus: text8 (Matt Mahoney) -- first 10^8 bytes of the English Wikipedia dump
(2006-03-03), cleaned to lowercase a-z + single spaces only. General English,
unrelated to any cipher test corpus.

Alphabet: 27 symbols -- A-Z (0..25) and SPACE (26).

Normalization (generic; a no-op for text8 which is already clean):
  * letters -> uppercase symbol, appended to the sliding window
  * whitespace runs -> single SPACE symbol (collapsed; leading whitespace skipped)
  * any other character (punctuation, digits, etc.) -> window is RESET
    (5-grams never cross word boundaries / punctuation)

File format (little-endian):
  u32 magic = 0x3547524E ("5GRN")
  u32 count N
  f32 floor   (min lp over the table)
  f32 ceil    (max lp over the table)
  then N records sorted by packed key ascending:
    3 bytes packed key (base-27: key = ((((s0*27+s1)*27+s2)*27+s3)*27+s4),
                       little-endian; max 27^5-1 = 14,348,906 < 2^24)
    1 byte score = round(255 * (lp - floor) / (ceil - floor)), clipped [0,255]

Smoothing: Lidstone with lambda=1 (Laplace) over V = 27^5 possible 5-grams:
  p = (count + 1) / (total_tokens + 27^5)
  lp = ln(p)
"""
import math
import os
import struct
import sys
import zipfile
from collections import Counter

DATA_DIR = os.path.dirname(os.path.abspath(__file__))
CORPUS_ZIP = os.path.join(DATA_DIR, "corpus", "text8.zip")
CORPUS_MEMBER = "text8"
OUT_BIN = os.path.join(DATA_DIR, "fivegrams.bin")
OUT_META = os.path.join(DATA_DIR, "fivegrams.meta.txt")
OUT_COUNTS = os.path.join(DATA_DIR, "fivegrams_counts.txt")

SOURCE_URL = "https://mattmahoney.net/dc/text8.zip"
N_KEEP = 500_000
LAM = 1.0            # Lidstone lambda (Laplace)
V = 27 ** 5          # 14,348,907 possible 5-grams
MAGIC = 0x3547524E   # "5GRN"

# byte -> action: 0..25 letter symbol, 26 SPACE, 255 RESET (punct/digit/other)
_WS = {9: 26, 10: 26, 13: 26, 32: 26}  # tab, LF, CR, space
_xlate = bytearray(255 for _ in range(256))
for b in range(ord("a"), ord("z") + 1):
    _xlate[b] = b - ord("a")
for b in range(ord("A"), ord("Z") + 1):
    _xlate[b] = b - ord("A")
for b, s in _WS.items():
    _xlate[b] = s
XLATE = bytes(_xlate)

SYM = [chr(ord("A") + i) for i in range(26)] + [" "]


def decode(key):
    chars = []
    for _ in range(5):
        chars.append(SYM[key % 27])
        key //= 27
    return "".join(reversed(chars))


def pack(s):
    k = 0
    for x in s:
        k = k * 27 + x
    return k


def main():
    print(f"reading corpus from {CORPUS_ZIP}", flush=True)
    counter = Counter()
    win = []          # sliding window of symbol ids
    total = 0         # total 5-gram tokens
    nbytes = 0
    with zipfile.ZipFile(CORPUS_ZIP) as zf, zf.open(CORPUS_MEMBER) as f:
        while True:
            chunk = f.read(1 << 20)
            if not chunk:
                break
            nbytes += len(chunk)
            for s in chunk.translate(XLATE):
                if s == 255:
                    # punctuation / digit / other: reset window, no crossing
                    win.clear()
                    continue
                if s == 26:
                    # whitespace: single SPACE between words, collapse runs,
                    # skip leading space
                    if not win or win[-1] == 26:
                        continue
                win.append(s)
                if len(win) == 5:
                    counter[pack(win)] += 1
                    total += 1
                    del win[0]
    distinct = len(counter)
    print(f"bytes={nbytes} tokens={total} distinct={distinct}", flush=True)

    keep = min(N_KEEP, distinct)
    top = counter.most_common(keep)
    print(f"keeping top {keep} 5-grams", flush=True)

    denom = total + LAM * V
    log_denom = math.log(denom)
    # lp is monotonic in count: floor from min count kept, ceil from max
    c_min = top[-1][1]
    c_max = top[0][1]
    floor = math.log(c_min + LAM) - log_denom
    ceil = math.log(c_max + LAM) - log_denom
    span = ceil - floor

    records = []
    for key, c in top:
        lp = math.log(c + LAM) - log_denom
        score = int(round(255.0 * (lp - floor) / span))
        score = max(0, min(255, score))
        records.append((key, score))
    records.sort(key=lambda r: r[0])
    assert len({k for k, _ in records}) == keep

    with open(OUT_BIN, "wb") as f:
        f.write(struct.pack("<II", MAGIC, keep))
        f.write(struct.pack("<ff", floor, ceil))
        for key, score in records:
            f.write(key.to_bytes(3, "little"))
            f.write(bytes((score,)))
    size = os.path.getsize(OUT_BIN)

    with open(OUT_COUNTS, "w") as f:
        for key, c in top[:20]:
            f.write(f"{decode(key)} {c}\n")

    meta = (
        "fivegrams.bin metadata\n"
        f"source URL: {SOURCE_URL}\n"
        "source: text8 (Matt Mahoney) -- first 100,000,000 bytes of the English\n"
        "        Wikipedia dump of 2006-03-03, cleaned: only lowercase a-z and\n"
        "        single spaces between words. General English; unrelated to any\n"
        "        cipher test corpus.\n"
        "date built: 2026-09-23\n"
        "normalization: letters -> uppercase; whitespace runs collapsed to one\n"
        "        SPACE; punctuation/digits/other reset the 5-gram window\n"
        "        (no 5-gram crosses a word boundary).\n"
        f"corpus bytes: {nbytes}\n"
        f"total 5-gram tokens counted: {total}\n"
        f"distinct 5-grams seen: {distinct}\n"
        f"5-grams kept: {keep}\n"
        "smoothing: Lidstone lambda=1 (Laplace), V = 27^5 = 14348907\n"
        "  lp = ln((count + 1) / (total + 27^5))\n"
        f"floor (min lp in table): {floor!r}\n"
        f"ceil (max lp in table): {ceil!r}\n"
        "layout: u32 magic 0x3547524E, u32 count, f32 floor, f32 ceil,\n"
        "        then N records sorted by key ascending: 3-byte base-27 packed\n"
        "        key (LE) + 1-byte score = round(255*(lp-floor)/(ceil-floor)).\n"
        f"byte size: {size}\n"
    )
    with open(OUT_META, "w") as f:
        f.write(meta)

    print(f"wrote {OUT_BIN} ({size} bytes), {OUT_META}, {OUT_COUNTS}")
    print(f"floor={floor!r} ceil={ceil!r}")
    print("top 5:", [(decode(k), c) for k, c in top[:5]])


if __name__ == "__main__":
    sys.exit(main())
