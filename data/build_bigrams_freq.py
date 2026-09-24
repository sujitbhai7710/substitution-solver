#!/usr/bin/env python3
"""Build a frequency-ranked compact bigram table for WASM.

Unlike build_bigrams_50k.py (top-N by conditional qscore, which favors rare
collocations), this keeps the top-N by raw COUNT (frequency) where both words
are in the 50k vocabulary. Frequent pairs ("OF THE", "IN THE") are the ones
that actually occur in puzzles and discriminate keys.

Same 50k-format layout as word_bigrams_50k.bin ("BRG3").
Writes to DST (caller decides the filename).
"""
import math
import struct
import sys
import zipfile
from collections import Counter

TEXT8 = "corpus/text8.zip"
WORDS50K = "words_50k.bin"
WORDS_FULL = "words.bin"
DST = sys.argv[1] if len(sys.argv) > 1 else "word_bigrams_50k_freq.bin"
N = int(sys.argv[2]) if len(sys.argv) > 2 else 60000
MAGIC = 0x35475242


def load_vocab(path):
    b = open(path, "rb").read()
    magic, count = struct.unpack("<II", b[:8])
    words = []
    pos = 20
    for _ in range(count):
        ln = b[pos]
        pos += 1
        words.append(b[pos:pos + ln].decode("ascii"))
        pos += ln + 1
    return words


v50k = load_vocab(WORDS50K)
vfull = load_vocab(WORDS_FULL)
idx50k = {w: i for i, w in enumerate(v50k)}
vfull_set = set(vfull)
print(f"50k vocab: {len(v50k)}, full vocab: {len(vfull)}", flush=True)

bigram_counts = Counter()
unigram_counts = Counter()
with zipfile.ZipFile(TEXT8) as z:
    name = z.namelist()[0]
    with z.open(name) as f:
        prev = None
        buf = b""
        while True:
            chunk = f.read(1 << 20)
            if not chunk:
                break
            buf += chunk
            parts = buf.split(b" ")
            buf = parts.pop()
            for pw in parts:
                w = pw.decode("ascii", "ignore").upper()
                if w and w in vfull_set:
                    unigram_counts[w] += 1
                    if prev is not None:
                        bigram_counts[(prev, w)] += 1
                    prev = w
                else:
                    prev = None
print(f"bigrams seen: {len(bigram_counts)}", flush=True)

# Top N by count, both words in 50k vocab
kept = []
for (w1, w2), c in bigram_counts.most_common():
    i1 = idx50k.get(w1)
    i2 = idx50k.get(w2)
    if i1 is not None and i2 is not None:
        kept.append((c, w1, w2, i1, i2))
        if len(kept) >= N:
            break
print(f"kept {len(kept)} by frequency", flush=True)

# Laplace P(w2|w1), same quantization params as the full table for
# comparability: reuse floor/ceil from word_bigrams.bin
b = open("word_bigrams.bin", "rb").read()
floor, ceil = struct.unpack("<ff", b[8:16])
oov = struct.unpack("<f", b[16 + 200000 * 9:16 + 200000 * 9 + 4])[0]
scale = (ceil - floor) / 255.0
V = len(vfull)


def qscore(lp):
    return max(0, min(255, int(round((lp - floor) / scale))))


rows = []
for c, w1, w2, i1, i2 in kept:
    lp = math.log((c + 1) / (unigram_counts[w1] + V))
    rows.append((qscore(lp), i1, i2))
rows.sort(key=lambda t: (t[1], t[2]))

out = bytearray()
out += struct.pack("<IIff", MAGIC, len(rows), floor, ceil)
for q, i1, i2 in rows:
    out += struct.pack("<HHB", i1, i2, q)
out += struct.pack("<f", oov)
open(DST, "wb").write(out)
print(f"wrote {DST}: {len(out)} bytes", flush=True)
