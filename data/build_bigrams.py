#!/usr/bin/env python3
"""Build data/word_bigrams.bin: top-N word bigram log-probs from text8.

For contextual re-ranking (e.g. "LIFE WILL" vs "LIKE WILL").

Layout:
  u32 magic 0x32475242 ("BRG2")
  u32 count
  f32 floor, f32 ceil  (quantization range for scores)
  per bigram: u32 w1_id, u32 w2_id, u8 qscore
  (w1_id/w2_id are indices into the words.bin vocabulary; bigrams with
   OOV words are skipped)
  Then: u32 oov_count, f32 oov_score (for unseen bigrams)

Bigram logp: ln((c+1)/(C1+V)) where C1 is unigram count of w1, V=vocab size.
This is P(w2|w1) with Laplace smoothing.
"""
import math
import struct
import sys
import zipfile
from collections import Counter

TEXT8 = "corpus/text8.zip"
WORDS_BIN = "words.bin"
DST = "word_bigrams.bin"
N = int(sys.argv[1]) if len(sys.argv) > 1 else 200000
MAGIC = 0x32475242

# Load vocabulary (word -> id)
vocab = {}
with open(WORDS_BIN, "rb") as f:
    b = f.read()
magic, count, oov, qfloor, qscale = struct.unpack("<IIfff", b[:20])
pos = 20
for i in range(count):
    ln = b[pos]; pos += 1
    w = b[pos:pos+ln].decode("ascii"); pos += ln
    pos += 1  # qscore
    vocab[w] = i
print(f"vocab: {len(vocab)} words")

# Count bigrams from text8
bigram_counts = Counter()
unigram_counts = Counter()
with zipfile.ZipFile(TEXT8) as z:
    name = z.namelist()[0]
    with z.open(name) as f:
        # text8 is space-separated lowercase words
        prev = None
        buf = b""
        while True:
            chunk = f.read(1 << 20)
            if not chunk:
                break
            buf += chunk
            parts = buf.split(b" ")
            buf = parts.pop()  # incomplete word
            for pw in parts:
                w = pw.decode("ascii", "ignore").upper()
                if w and w in vocab:
                    unigram_counts[w] += 1
                    if prev is not None:
                        bigram_counts[(prev, w)] += 1
                    prev = w
                else:
                    prev = None  # reset on OOV/punct
print(f"bigrams seen: {len(bigram_counts)}, tokens: {sum(bigram_counts.values())}")

# Keep top N by count
top = bigram_counts.most_common(N)
print(f"keeping top {len(top)}")

V = len(vocab)
# Compute P(w2|w1) with Laplace: (c+1)/(C1+V)
scores = []
for (w1, w2), c in top:
    c1 = unigram_counts[w1]
    lp = math.log((c + 1) / (c1 + V))
    scores.append((vocab[w1], vocab[w2], lp))

floor = min(s for _, _, s in scores)
ceil = max(s for _, _, s in scores)
scale = (ceil - floor) / 255.0
oov_score = math.log(1.0 / V)  # unseen bigram: uniform-ish

def q(lp):
    return max(0, min(255, int(round((lp - floor) / scale))))

out = bytearray()
out += struct.pack("<IIff", MAGIC, len(scores), floor, ceil)
# Sort by (w1, w2) for binary search
scores.sort()
for w1, w2, lp in scores:
    out += struct.pack("<IIB", w1, w2, q(lp))
out += struct.pack("<f", oov_score)

with open(DST, "wb") as f:
    f.write(out)
print(f"wrote {DST}: {len(out)} bytes, floor={floor:.2f} ceil={ceil:.2f} oov={oov_score:.2f}")
