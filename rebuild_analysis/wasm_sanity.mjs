import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const pkgDir = join(__dirname, 'www/pkg');
const wasmBytes = readFileSync(join(pkgDir, 'subst_solver_bg.wasm'));
console.log('WASM bytes:', wasmBytes.length);

// Shim fetch so the generated ESM bindings load the wasm from disk.
globalThis.fetch = async (url) => new Response(wasmBytes, {
  headers: { 'Content-Type': 'application/wasm' },
});

const { default: init, solve_js } = await import('./www/pkg/subst_solver.js');
await init();

function test(name, puzzle, clue, restarts, steps, check) {
  const out = JSON.parse(solve_js(puzzle, clue, restarts, steps));
  const ok = check(out);
  console.log((ok ? 'PASS' : 'FAIL') + ' | ' + name);
  if (!ok) console.log('  output:', JSON.stringify(out.solution ?? out));
  return ok;
}

let allOk = true;
allOk &= test('curly apostrophe', 'DON\u2019T', '', 12, 6000, (o) => o.plaintext.includes('\u2019'));
allOk &= test('clue lock', 'ABC', 'A=X', 12, 6000, (o) => o.plaintext[0] === 'X');
allOk &= test('whitespace', 'A  B\n\tC', '', 12, 6000, (o) => o.plaintext.includes('  ') && o.plaintext.includes('\n') && o.plaintext.includes('\t'));
allOk &= test('dashes', 'A \u2014 B \u2013 C', '', 12, 6000, (o) => o.plaintext.includes('\u2014') && o.plaintext.includes('\u2013'));
allOk &= test('casing', 'AbC', '', 12, 6000, (o) => /^[A-Za-z]{3}$/.test(o.plaintext) && o.plaintext[1] === o.plaintext[1].toLowerCase());
allOk &= test('real solve', 'ZNK YOXZ DMJBG QVGTJ VQ ZNK ODAWJTJ DMJBG ZXG UQOD OJ QVGTJ.', '', 12, 6000, (o) => o.plaintext.toUpperCase().includes('THE'));

console.log(allOk ? 'ALL SANITY TESTS PASSED' : 'SOME TESTS FAILED');
process.exit(allOk ? 0 : 1);
