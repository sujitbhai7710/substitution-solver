#!/usr/bin/env python3
"""Build data/words.bin: compact binary wordlist for the solver.

Takes the top N words by frequency from wordlist.tsv and stores:
  u32 magic 0x57524431 ("WRD1")
  u32 count
  f32 oov_logp            (out-of-vocabulary log-prob)
  f32 qfloor, f32 qscale  (u8 quantization of per-word logp)
  per word: u8 len, len bytes (uppercase ASCII + apostrophe), u8 qlogp

Log-probs use the same add-0.5 smoothing as the solver's TSV loader, computed
over the truncated set.
"""
import math
import struct
import sys

SRC = "wordlist.tsv"
DST = "words.bin"
MAGIC = 0x57524431
N = int(sys.argv[1]) if len(sys.argv) > 1 else 20000

words = []
with open(SRC) as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        parts = line.split("\t")
        if len(parts) != 2:
            parts = line.split()
        if len(parts) != 2:
            continue
        w, fs = parts
        w = w.strip().upper()
        try:
            fr = float(fs)
        except ValueError:
            continue
        if fr <= 0 or not w:
            continue
        if not all(c.isascii() and (c.isupper() or c == "'") for c in w):
            continue
        words.append((w, fr))
        if len(words) >= N:
            break

assert len(words) > 10000, "wordlist too small"
total = sum(fr for _, fr in words)
v = len(words)
alpha = 0.5
denom = total + alpha * v
oov = math.log(alpha / denom)
lps = [math.log((fr + alpha) / denom) for _, fr in words]
floor = min(lps)
ceil = max(lps)
scale = (ceil - floor) / 255.0


def q(lp):
    return max(0, min(255, int(round((lp - floor) / scale))))


out = bytearray()
out += struct.pack("<IIfff", MAGIC, len(words), oov, floor, scale)
for (w, _), lp in zip(words, lps):
    wb = w.encode("ascii")
    assert len(wb) < 256
    out += struct.pack("B", len(wb)) + wb + struct.pack("B", q(lp))

with open(DST, "wb") as f:
    f.write(out)
print(f"words: {len(words)}  oov={oov:.3f} floor={floor:.3f} ceil={ceil:.3f}")
print(f"wrote {DST}: {len(out)} bytes")
