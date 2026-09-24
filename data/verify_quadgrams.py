#!/usr/bin/env python3
"""Verify quadgrams.f32: size, spot checks, floor value."""
import math
import os
import struct

PATH = "/home/hatch/workspace/substitution-solver/data/quadgrams.f32"
SRC = "/tmp/english_quadgrams.txt"

size = os.path.getsize(PATH)
assert size == 1827904, f"size mismatch: {size}"
print(f"size OK: {size} bytes")

with open(PATH, "rb") as f:
    data = f.read()
n = len(data) // 4
assert n == 26 ** 4 == 456976, n

def logp(quad):
    idx = ((ord(quad[0]) - 65) * 26 + (ord(quad[1]) - 65)) * 26 * 26 \
        + ((ord(quad[2]) - 65) * 26 + (ord(quad[3]) - 65))
    # index = ((a*26+b)*26+c)*26+d
    a, b, c, d = (ord(ch) - 65 for ch in quad)
    idx = ((a * 26 + b) * 26 + c) * 26 + d
    return struct.unpack_from("<f", data, idx * 4)[0]

# Recompute expected values independently from the source file
counts, total = {}, 0
with open(SRC) as f:
    for line in f:
        q, c = line.split()
        counts[q] = int(c)
        total += int(c)
expected_floor = math.log(0.01 / total)
for quad in ("TION", "THAT", "THER", "ZXQJ"):
    exp = (math.log(counts[quad] / total) if quad in counts else expected_floor)
    got = logp(quad)
    match = math.isclose(got, exp, rel_tol=1e-6)
    print(f"{quad}: stored={got:.6f} expected={exp:.6f} match={match}")
    assert match, quad

floor_got = logp("ZXQJ")
assert math.isclose(floor_got, expected_floor, rel_tol=1e-6)
print(f"floor OK: {floor_got:.6f} == ln(0.01/{total})")

# common quadgrams must be far above floor
for quad in ("TION", "THAT", "THER"):
    assert logp(quad) > floor_got + 10, quad
print("common quadgrams >> floor: OK")

# no NaN / +inf; values must be finite (floor is the minimum for unseen)
import array
arr = array.array("f")
arr.frombytes(data)
assert len(arr) == 456976
mn, mx = min(arr), max(arr)
assert all(math.isfinite(v) for v in (mn, mx))
print(f"value range: min={mn:.4f} max={mx:.4f} (all finite)")
print("ALL CHECKS PASSED")
