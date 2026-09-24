import subprocess, json, sys
items = json.load(open('/home/hatch/workspace/substitution-solver/rebuild/bench/sample90.json'))
BIN = '/tmp/target-wasmtables/release/examples/solve_one'
def norm(s):
    return ''.join(c for c in s.upper() if c.isalpha() and c.isascii())
out = []
for i, p in enumerate(items):
    r = subprocess.run([BIN], input=p['puzzle'].encode(), capture_output=True, timeout=120, env={**__import__('os').environ})
    txt = r.stdout.decode(errors='replace').strip().split('\n')[0]
    ok = norm(txt) == norm(p['answer'])
    out.append({'i': i, 'type': p['type'], 'ok': ok, 'out': txt[:60]})
json.dump(out, open(sys.argv[1], 'w'))
print('native correct:', sum(1 for o in out if o['ok']), '/', len(out))
