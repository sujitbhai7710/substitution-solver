import { solve_js, warmup_js } from './www/pkg_node/subst_solver.js';
import fs from 'fs';

const items = JSON.parse(fs.readFileSync(process.argv[4] || 'bench/sample300.json', 'utf8'));
const R = parseInt(process.argv[2] || '48');
const S = parseInt(process.argv[3] || '12000');

warmup_js();

const norm = s => s.toUpperCase().replace(/[^A-Z]/g, '');
let correct = { 'cryptoquip': 0, 'cryptoquote': 0, 'celebrity-cipher': 0 };
let total = { 'cryptoquip': 0, 'cryptoquote': 0, 'celebrity-cipher': 0 };
let msAll = [];
const t0 = Date.now();
for (const p of items) {
  const t = Date.now();
  const r = JSON.parse(solve_js(p.puzzle, '', R, S));
  const ms = Date.now() - t;
  msAll.push(ms);
  total[p.type]++;
  if (norm(r.plaintext) === norm(p.answer)) correct[p.type]++;
}
msAll.sort((a, b) => a - b);
for (const t of ['cryptoquip', 'cryptoquote', 'celebrity-cipher']) {
  console.log(`${t}: ${correct[t]}/${total[t]} = ${(correct[t]/total[t]*100).toFixed(1)}%`);
}
const n = msAll.length, c = correct['cryptoquip'] + correct['cryptoquote'] + correct['celebrity-cipher'];
console.log(`OVERALL: ${c}/${n} = ${(c/n*100).toFixed(2)}%  median ${msAll[n>>1].toFixed(0)}ms  wall ${((Date.now()-t0)/1000).toFixed(0)}s`);
