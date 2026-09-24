#!/usr/bin/env python3
"""Build data/word_bigrams_50k.bin: compact bigram table for WASM.

Keeps the most predictive bigrams whose words are both in the 50k vocabulary,
remapped to 50k u16 word IDs.

Layout:
  u32 magic 0x35475242 ("BRG3")
  u32 count
  f32 floor, f32 ceil  (same quantization as word_bigrams.bin)
  per bigram: u16 w1_50k, u16 w2_50k, u8 qscore   (5 bytes)
  f32 oov_score
"""
import struct
import sys

SRC = "data/word_bigrams.bin"
WORDS50K = "data/words_50k.bin"
WORDS_FULL = "data/words.bin"
DST = "data/word_bigrams_50k.bin"
N = int(sys.argv[1]) if len(sys.argv) > 1 else 60000
MAGIC = 0x35475242

def load_vocab(path):
    b = open(path, "rb").read()
    magic, count, oov, qfloor, qscale = struct.unpack("<IIfff", b[:20])
    assert magic == 0x57524431, hex(magic)
    words = []
    pos = 20
    for _ in range(count):
        ln = b[pos]; pos += 1
        w = b[pos:pos+ln].decode("ascii"); pos += ln
        pos += 1  # qlogp
        words.append(w)
    return words

v50k = load_vocab(WORDS50K)
vfull = load_vocab(WORDS_FULL)
idx50k = {w: i for i, w in enumerate(v50k)}
print(f"50k vocab: {len(v50k)}, full vocab: {len(vfull)}")

b = open(SRC, "rb").read()
magic, count = struct.unpack("<II", b[:8])
assert magic == 0x32475242, hex(magic)
floor, ceil = struct.unpack("<ff", b[8:16])
print(f"src bigrams: {count}, floor={floor:.2f} ceil={ceil:.2f}")
oov = struct.unpack("<f", b[16 + count*9:16 + count*9 + 4])[0]

kept = []  # (qscore, w1_50k, w2_50k)
pos = 16
for _ in range(count):
    w1, w2, q = struct.unpack("<IIB", b[pos:pos+9])
    pos += 9
    s1, s2 = vfull[w1], vfull[w2]
    i1 = idx50k.get(s1)
    i2 = idx50k.get(s2)
    if i1 is not None and i2 is not None:
        kept.append((q, i1, i2))
print(f"both in 50k: {len(kept)}")

# Keep top N by qscore (most predictive / highest P(w2|w1))
kept.sort(reverse=True)
kept = kept[:N]
# Sort by (w1, w2) for binary search
kept.sort(key=lambda t: (t[1], t[2]))
print(f"keeping top {len(kept)}, min q={kept[-1][0]}")

out = bytearray()
out += struct.pack("<IIff", MAGIC, len(kept), floor, ceil)
for q, i1, i2 in kept:
    out += struct.pack("<HHB", i1, i2, q)
out += struct.pack("<f", oov)
open(DST, "wb").write(out)
print(f"wrote {DST}: {len(out)} bytes")
