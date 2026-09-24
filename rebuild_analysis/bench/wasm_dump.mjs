import { solve_js, warmup_js } from '../www/pkg_node/subst_solver.js';
import fs from 'fs';
const items = JSON.parse(fs.readFileSync('bench/sample90.json', 'utf8'));
warmup_js();
const norm = s => s.toUpperCase().replace(/[^A-Z]/g, '');
const out = [];
for (let i = 0; i < items.length; i++) {
  const p = items[i];
  const r = JSON.parse(solve_js(p.puzzle, '', 48, 12000));
  out.push({ i, type: p.type, ok: norm(r.plaintext) === norm(p.answer), out: r.plaintext.slice(0, 60) });
}
fs.writeFileSync(process.argv[2], JSON.stringify(out));
console.log('wasm correct:', out.filter(o => o.ok).length, '/', out.length);
