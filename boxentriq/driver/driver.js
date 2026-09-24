// Black-box benchmark driver for boxentriq's substitution-cipher solver.
//
// Runs boxentriq's ma_worker.js (webpack bundle around a Rust WASM solver)
// inside a Node worker_thread, inits it once with the UI defaults, then
// solves every puzzle in the test corpus sequentially, timing each solve.
// Results: boxentriq_results.jsonl  (one JSON object per line)
//
// Usage:
//   node driver.js [--limit N] [--resume] [--out PATH] [--init-timeout MS] [--solve-timeout MS]
//
// No boxentriq code, algorithms, or data are copied into our own solver:
// this driver only talks to the worker through its public message protocol.
'use strict';
const fs = require('fs');
const path = require('path');
const { Worker } = require('worker_threads');
const { start: startServer } = require('./server');

const DRIVER_DIR = __dirname;
const CORPUS_PATH = process.env.BOX_CORPUS || '/home/hatch/workspace/substitution-solver/test-corpus/corpus.json';

// --- Exact boxentriq UI defaults (verified from page.html + substitution-cipher.js) ---
const SETTINGS = {
  // init
  langCode: 'en',
  alphabetLetters: 'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
  lowMemory: true, // "Analysis quality" select default = "Standard" (value "true") -> solverIntelligence=true -> lowMemory=true -> 4-gram profiles
  // solve
  delimiterMode: 0, // "Spacing mode" select default = "Automatic" (value 0)
  crib: '', // "Partial text" input default empty
  cribWeight: 0.3, // hardcoded in substitution-cipher.js solve message
  keyTemplate: '', // no manual key letters entered
  potentialDelimiterSubstitutes: 'X', // en language definition in main.js
  numberOfResults: 5, // "Results to keep" input default
  numberOfRounds: 100, // ceil(sqrt(searchDepth)), searchDepth default = 10000
  numberOfSteps: 10000, // "Search depth" input default
};

function parseArgs(argv) {
  const a = { limit: Infinity, resume: false, out: path.join(DRIVER_DIR, 'boxentriq_results.jsonl'), initTimeoutMs: 180000, solveTimeoutMs: 600000 };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--limit') a.limit = parseInt(argv[++i], 10);
    else if (argv[i] === '--resume') a.resume = true;
    else if (argv[i] === '--out') a.out = argv[++i];
    else if (argv[i] === '--init-timeout') a.initTimeoutMs = parseInt(argv[++i], 10);
    else if (argv[i] === '--solve-timeout') a.solveTimeoutMs = parseInt(argv[++i], 10);
  }
  return a;
}

class BoxSolver {
  constructor(origin, opts) {
    this.origin = origin;
    this.opts = opts;
    this.worker = null;
    this.pending = null; // {resolve, reject, timer, want}
  }

  spawn() {
    return new Promise((resolve, reject) => {
      const w = new Worker(path.join(DRIVER_DIR, 'worker_bootstrap.js'), {
        workerData: { origin: this.origin },
      });
      const onErr = (e) => reject(new Error('worker failed to start: ' + (e && e.message)));
      w.once('error', onErr);
      w.once('online', () => { w.off('error', onErr); resolve(w); });
      w.on('message', (msg) => this._onMessage(msg));
      w.on('error', (e) => this._onWorkerError(e));
      w.on('exit', (code) => this._onWorkerExit(code));
      this.worker = w;
    });
  }

  _onMessage(msg) {
    if (!msg || typeof msg !== 'object') return;
    const p = this.pending;
    if (!p) return; // unsolicited (shouldn't happen with sequential solves)
    if (msg.kind === 'initialized' && p.want === 'initialized') {
      this._settle(null, msg);
    } else if (msg.kind === 'results' && p.want === 'results') {
      if (msg.payload && msg.payload.isFinished) this._settle(null, msg);
      // else: intermediate progress message; keep waiting
    } else if (msg.kind === 'error' && (p.want === 'results' || p.want === 'initialized')) {
      const m = (msg.payload && msg.payload.message) || 'unknown worker error';
      this._settle(new Error('worker error: ' + m), null);
    }
    // 'status' messages ignored
  }

  _settle(err, msg) {
    const p = this.pending;
    if (!p) return;
    this.pending = null;
    clearTimeout(p.timer);
    if (err) p.reject(err); else p.resolve(msg);
  }

  _onWorkerError(e) {
    this._settle(new Error('worker thread error: ' + (e && e.message)), null);
  }

  _onWorkerExit(code) {
    if (this.pending) this._settle(new Error('worker exited with code ' + code), null);
  }

  _sendAndWait(msg, want, timeoutMs) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending = null;
        reject(new Error(`timeout waiting for ${want} after ${timeoutMs}ms`));
      }, timeoutMs);
      this.pending = { resolve, reject, timer, want };
      this.worker.postMessage(msg);
    });
  }

  async init() {
    await this.spawn();
    await this._sendAndWait(
      { id: 'init', langCode: SETTINGS.langCode, alphabetLetters: SETTINGS.alphabetLetters, lowMemory: SETTINGS.lowMemory },
      'initialized', this.opts.initTimeoutMs);
  }

  async solve(cipherText) {
    const msg = await this._sendAndWait({
      id: 'solve',
      cipherText,
      delimiterMode: SETTINGS.delimiterMode,
      crib: SETTINGS.crib,
      cribWeight: SETTINGS.cribWeight,
      keyTemplate: SETTINGS.keyTemplate,
      potentialDelimiterSubstitutes: SETTINGS.potentialDelimiterSubstitutes,
      numberOfResults: SETTINGS.numberOfResults,
      numberOfRounds: SETTINGS.numberOfRounds,
      numberOfSteps: SETTINGS.numberOfSteps,
    }, 'results', this.opts.solveTimeoutMs);
    const results = (msg.payload && msg.payload.results) || [];
    const top = results[0] || null;
    const plaintext = top ? (top.segmentedPlaintext || top.plaintext || null) : null;
    return { plaintext, nResults: results.length, rawTop: top };
  }

  async terminate() {
    this._settle(new Error('solver terminated'), null);
    if (this.worker) { try { await this.worker.terminate(); } catch (_) {} this.worker = null; }
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const corpus = JSON.parse(fs.readFileSync(CORPUS_PATH, 'utf8'));
  console.log(`corpus: ${corpus.length} entries from ${CORPUS_PATH}`);

  const done = new Set();
  if (args.resume && fs.existsSync(args.out)) {
    for (const line of fs.readFileSync(args.out, 'utf8').split('\n')) {
      if (!line.trim()) continue;
      try {
        const r = JSON.parse(line);
        done.add(r.type + '|' + r.date + '|' + r.puzzle);
      } catch (_) {}
    }
    console.log(`resume: skipping ${done.size} already-solved entries`);
  }
  const out = fs.createWriteStream(args.out, { flags: args.resume ? 'a' : 'w' });

  const { server, origin } = await startServer(0);
  console.log(`harness server: ${origin}`);

  const solver = new BoxSolver(origin, args);
  console.log('initializing boxentriq worker...');
  const tInit0 = Date.now();
  await solver.init();
  console.log(`worker initialized in ${Date.now() - tInit0}ms`);

  fs.writeFileSync(path.join(DRIVER_DIR, 'settings.json'), JSON.stringify(SETTINGS, null, 2));
  console.log('settings:', JSON.stringify(SETTINGS));

  let solved = 0, failed = 0, skipped = 0;
  const tAll0 = Date.now();
  const entries = corpus.slice(0, args.limit);
  for (let i = 0; i < entries.length; i++) {
    const e = entries[i];
    const key = e.type + '|' + e.date + '|' + e.puzzle;
    if (done.has(key)) { skipped++; continue; }
    const t0 = Date.now();
    let box_plaintext = null, error = null;
    try {
      const r = await solver.solve(e.puzzle);
      box_plaintext = r.plaintext;
      if (box_plaintext == null) error = `no results (n=${r.nResults})`;
    } catch (err) {
      error = err.message;
      // If the worker is wedged (timeout/exit), rebuild it and re-init so the
      // rest of the run isn't poisoned by a late result for the wrong puzzle.
      try { await solver.terminate(); } catch (_) {}
      try { await solver.init(); } catch (e2) { error += ` | reinit failed: ${e2.message}`; }
    }
    const ms = Date.now() - t0;
    const rec = { type: e.type, date: e.date, puzzle: e.puzzle, answer: e.answer, box_plaintext, box_ms: ms };
    if (error) { rec.error = error; failed++; } else solved++;
    out.write(JSON.stringify(rec) + '\n');
    if ((solved + failed) % 25 === 0 || i === entries.length - 1) {
      const el = ((Date.now() - tAll0) / 1000).toFixed(0);
      console.log(`[${solved + failed + skipped}/${entries.length}] solved=${solved} failed=${failed} skipped=${skipped} elapsed=${el}s`);
    }
  }
  out.end();
  await new Promise((r) => out.on('finish', r));
  await solver.terminate();
  server.close();
  const totalS = (Date.now() - tAll0) / 1000;
  console.log(`DONE: solved=${solved} failed=${failed} skipped=${skipped} total=${totalS.toFixed(1)}s (${(solved / Math.max(totalS, 1)).toFixed(2)} puzzles/s)`);
  console.log(`results: ${args.out}`);
}

main().catch((e) => { console.error('FATAL:', e); process.exit(1); });
