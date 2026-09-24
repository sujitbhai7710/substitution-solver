import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgDir = join(__dirname, 'www/pkg');
const wasmBytes = readFileSync(join(pkgDir, 'subst_solver_bg.wasm'));

globalThis.fetch = async (url) => new Response(wasmBytes, {
  headers: { 'Content-Type': 'application/wasm' },
});

const { default: init, solve_js } = await import('./www/pkg/subst_solver.js');
await init();

const dev = JSON.parse(readFileSync(join(__dirname, '..', 'test-corpus', 'dev.json'), 'utf8'));
let ok = 0;
const times = [];
const perType = {};
for (const e of dev) {
  const t = Date.now();
  const out = JSON.parse(solve_js(e.puzzle, '', 12, 6000));
  const ms = Date.now() - t;
  times.push(ms);
  const hit = out.plaintext === e.answer;
  if (hit) ok++;
  const s = perType[e.type] ??= { n: 0, ok: 0, ms: [] };
  s.n++; if (hit) s.ok++; s.ms.push(ms);
}
times.sort((a, b) => a - b);
console.log(`round4-wasm dev set: ${ok}/${dev.length} = ${(100*ok/dev.length).toFixed(2)}% exact, median ${(times[times.length>>1]/1000).toFixed(2)}s`);
for (const [t, s] of Object.entries(perType)) {
  s.ms.sort((a, b) => a - b);
  console.log(`  ${t}: ${s.ok}/${s.n} = ${(100*s.ok/s.n).toFixed(1)}%, median ${(s.ms[s.ms.length>>1]/1000).toFixed(2)}s`);
}
