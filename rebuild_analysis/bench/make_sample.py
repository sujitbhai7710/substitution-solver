#!/usr/bin/env python3
"""Regenerate deterministic benchmark samples from the persistent corpus.

Procedure (2026-09-23, after /tmp scratch was cleaned):
  random.seed(42); for each of the 3 types in order
  [cryptoquip, cryptoquote, celebrity-cipher], take random.sample(items, 100).
Writes sample300.json (300) and sample90.json (first 30 of each type).
"""
import json
import random

random.seed(42)
corpus = json.load(open('/home/hatch/workspace/substitution-solver/test-corpus/corpus.json'))
sample = []
for t in ['cryptoquip', 'cryptoquote', 'celebrity-cipher']:
    items = [x for x in corpus if x['type'] == t]
    sample.extend(random.sample(items, 100))

with open('sample300.json', 'w') as f:
    json.dump(sample, f, ensure_ascii=False)

sub = []
for t in ['cryptoquip', 'cryptoquote', 'celebrity-cipher']:
    sub.extend([x for x in sample if x['type'] == t][:30])
with open('sample90.json', 'w') as f:
    json.dump(sub, f, ensure_ascii=False)

print(f"sample300: {len(sample)}, sample90: {len(sub)}")
