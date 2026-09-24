#!/usr/bin/env python3
"""Build quadgrams.f32: flat table of 26^4 little-endian f32 natural-log probabilities."""
import math
import sys
import array

SRC = "/tmp/english_quadgrams.txt"
OUT = "/home/hatch/workspace/substitution-solver/data/quadgrams.f32"
META = "/home/hatch/workspace/substitution-solver/data/quadgrams.meta.txt"
SOURCE_URL = "https://raw.githubusercontent.com/vkkkv/subsolve/HEAD/data/english_quadgrams.txt"

assert sys.byteorder == "little", "need little-endian host"

counts = {}
total = 0
n_lines = 0
with open(SRC) as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        q, c = line.split()
        assert len(q) == 4 and q.isupper() and q.isalpha(), f"bad key: {q!r}"
        assert q not in counts, f"duplicate key: {q}"
        c = int(c)
        counts[q] = c
        total += c
        n_lines += 1

distinct = len(counts)
floor = math.log(0.01 / total)
log_total = math.log(total)

vals = array.array("f")  # native float; little-endian host => little-endian bytes
append = vals.append
get = counts.get
for a in range(26):
    ca = chr(65 + a)
    for b in range(26):
        cab = ca + chr(65 + b)
        for c in range(26):
            cabc = cab + chr(65 + c)
            for d in range(26):
                cnt = get(cabc + chr(65 + d))
                append(math.log(cnt) - log_total if cnt else floor)

assert len(vals) == 26 ** 4, len(vals)
with open(OUT, "wb") as f:
    vals.tofile(f)

meta = (
    "quadgrams.f32 metadata\n"
    f"source URL: {SOURCE_URL}\n"
    "source: english_quadgrams.txt (James Lyons / practicalcryptography.com quadgram\n"
    "        table, via vkkkv/subsolve GitHub repo data/english_quadgrams.txt; the direct\n"
    "        download link on the quadgrams page could not be fetched, so the repo copy\n"
    "        was used as the documented fallback)\n"
    "date fetched: 2026-09-23\n"
    f"source lines parsed: {n_lines}\n"
    f"total quadgram count (sum of counts): {total}\n"
    f"distinct quadgrams seen: {distinct}\n"
    "floor convention (Lyons): ln(0.01 / total) for unseen quadgrams\n"
    f"floor value: {floor!r}\n"
    "layout: 26^4 = 456976 little-endian f32 natural-log probabilities,\n"
    "        index = ((a*26+b)*26+c)*26+d for uppercase A-Z (a,b,c,d in 0..25)\n"
    "byte size: 1827904\n"
)
with open(META, "w") as f:
    f.write(meta)

print(f"lines={n_lines} distinct={distinct} total={total}")
print(f"floor={floor!r}")
print(f"logp(TION)={math.log(counts['TION']) - log_total!r}")
print("wrote", OUT, "and", META)
