#!/usr/bin/env python3
"""Benchmark solve_one on a sample JSON. 2 workers (2-core box).
Usage: python3 bench.py sample300.json   (env overrides: R S T0 TE WW BG TW NW BEAM)
"""
import json, subprocess, os, sys, time
from concurrent.futures import ThreadPoolExecutor
from collections import Counter

BIN = '/tmp/target-bench/release/examples/solve_one'
items = json.load(open(sys.argv[1]))

def norm(s):
    return ''.join(c for c in s.upper() if c.isalpha() and c.isascii())

ENV = dict(os.environ)
for k in ['R', 'S']:
    ENV.pop(k, None)

def one(p):
    t = time.time()
    try:
        r = subprocess.run([BIN], input=p['puzzle'].encode(),
                           capture_output=True, timeout=180, env=ENV)
        out = r.stdout.decode(errors='replace').strip().split('\n')[0]
        return p['type'], norm(out) == norm(p['answer']), (time.time() - t) * 1000
    except Exception:
        return p['type'], False, 0.0

t0 = time.time()
with ThreadPoolExecutor(max_workers=2) as ex:
    res = list(ex.map(one, items))
wall = time.time() - t0

by = Counter()
tot = Counter()
ms = {}
for t, ok, m in res:
    tot[t] += 1
    by[t] += ok
    ms.setdefault(t, []).append(m)
for t in ['cryptoquip', 'cryptoquote', 'celebrity-cipher']:
    arr = sorted(ms.get(t, [0]))
    med = arr[len(arr) // 2] if arr else 0
    print(f"{t}: {by[t]}/{tot[t]} = {by[t]/tot[t]*100:.1f}%  median {med:.0f}ms")
n = sum(tot.values())
c = sum(by.values())
allm = sorted(m for _, _, m in res)
print(f"OVERALL: {c}/{n} = {c/n*100:.2f}%  median {allm[n//2]:.0f}ms  wall {wall:.0f}s")
