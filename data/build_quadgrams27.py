#!/usr/bin/env python3
"""Build quadgrams27.q8: 27^4 quadgram log-probs over A-Z + space (26).

Corpus: text8 (100MB Wikipedia). Symbols: a-z -> 0..25, space -> 26,
any other byte -> 26 (space). No collapsing.

Layout: u32 magic 0x51444732 ("QDG2"), f32 floor, f32 scale,
then 27^4 = 531441 u8 quantized entries,
index = ((a*27+b)*27+c)*27+d.
Floor convention (Lyons): ln(0.01 / total) for unseen quadgrams.
"""
import math
import struct
import array
import sys

SRC = "/home/hatch/workspace/substitution-solver/data/corpus/text8"
OUT = "/home/hatch/workspace/substitution-solver/data/quadgrams27.q8"

N = 27 ** 4
counts = array.array("I", [0]) * N  # 2.1MB

total = 0
with open(SRC, "rb") as f:
    data = f.read()

print(f"corpus bytes: {len(data)}", flush=True)
syms = bytearray(len(data))
for i, ch in enumerate(data):
    if 97 <= ch <= 122:
        syms[i] = ch - 97
    else:
        syms[i] = 26
n = len(syms)
for i in range(n - 3):
    counts[(syms[i] * 27 + syms[i + 1]) * 27 * 27 + syms[i + 2] * 27 + syms[i + 3]] += 1
    total += 1

print(f"quadgrams counted: {total}", flush=True)
distinct = sum(1 for x in counts if x)
print(f"distinct: {distinct}", flush=True)

floor = math.log(0.01 / total)
log_total = math.log(total)
# find max logp for scale
maxlp = floor
for x in counts:
    if x:
        lp = math.log(x) - log_total
        if lp > maxlp:
            maxlp = lp
scale = (maxlp - floor) / 255.0
print(f"floor={floor!r} maxlp={maxlp!r} scale={scale!r}", flush=True)

out = bytearray()
out += struct.pack("<Iff", 0x51444732, floor, scale)
for x in counts:
    lp = math.log(x) - log_total if x else floor
    q = int(round((lp - floor) / scale))
    out.append(255 if q > 255 else q)

assert len(out) == 12 + N, len(out)
with open(OUT, "wb") as f:
    f.write(out)
print("wrote", OUT, len(out), "bytes")
