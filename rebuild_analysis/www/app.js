// Substitution Cipher Solver frontend.
// CRITICAL: the ciphertext is passed to WASM byte-for-byte unchanged.
// No normalization of curly quotes, dashes, casing, or whitespace happens
// here — the Rust engine preserves everything verbatim on output.

import init, { solve_js, warmup_js } from './pkg/subst_solver.js';

let wasmReady = false;

const el = (id) => document.getElementById(id);

function countLetters(s) {
  return (s.match(/[A-Za-z]/g) || []).length;
}

function updateStats() {
  const v = el('toolInput').value;
  el('inputStats').textContent = `${v.length} chars · ${countLetters(v)} letters`;
}

function setBusy(state, msg) {
  el('solveSpinner').classList.toggle('d-none', !state);
  el('autosolveButton').disabled = state;
  if (msg !== undefined) el('solveStatus').textContent = msg;
}

async function runSolver() {
  // Pass the raw textarea value straight through — no cleanup, no
  // case-folding, no punctuation normalization. The engine handles it.
  const puzzle = el('toolInput').value;
  if (countLetters(puzzle) < 4) {
    el('solveStatus').textContent = 'Please provide at least 4 letters.';
    return;
  }
  if (!wasmReady) {
    el('solveStatus').textContent = 'Engine still loading, try again in a moment.';
    return;
  }
  const clue = el('clueInput').value;
  const depth = parseInt(el('searchDepth').value, 10) || 12000;
  const quality = el('analysisQuality').value;
  const restarts = quality === 'high' ? 48 : 24;
  const steps = quality === 'high' ? Math.min(depth, 12000) : Math.min(depth, 6000);

  setBusy(true, 'Solving…');
  // Let the browser paint before the synchronous WASM call blocks.
  await new Promise((r) => setTimeout(r, 20));

  const t0 = performance.now();
  let result;
  try {
    // solve_js(puzzle, clue, restarts, steps) -> JSON string.
    result = JSON.parse(solve_js(puzzle, clue, restarts, steps));
  } catch (e) {
    console.error(e);
    setBusy(false, 'Solver error: ' + e.message);
    return;
  }
  const dt = performance.now() - t0;

  el('solverOutput').value = result.plaintext;
  el('solveMeta').textContent =
    `Solved in ${dt.toFixed(0)} ms (engine ${result.ms.toFixed(0)} ms) · score ${result.score.toFixed(1)}`;
  el('footerStats').textContent = `Last solve: ${dt.toFixed(0)} ms`;
  setBusy(false, 'Done.');
}

// Theme toggle
function initTheme() {
  const btn = el('themeToggle');
  btn.addEventListener('click', () => {
    const html = document.documentElement;
    const cur = html.getAttribute('data-bs-theme');
    html.setAttribute('data-bs-theme', cur === 'dark' ? 'light' : 'dark');
  });
}

async function main() {
  initTheme();
  el('toolInput').addEventListener('input', updateStats);
  el('autosolveButton').addEventListener('click', runSolver);
  el('clearInputBtn').addEventListener('click', () => {
    el('toolInput').value = '';
    updateStats();
  });
  el('pasteInputBtn').addEventListener('click', async () => {
    try {
      el('toolInput').value = await navigator.clipboard.readText();
      updateStats();
    } catch (e) {
      el('solveStatus').textContent = 'Clipboard read failed: ' + e.message;
    }
  });
  el('copyOutputBtn').addEventListener('click', () => {
    navigator.clipboard.writeText(el('solverOutput').value);
  });
  updateStats();

  try {
    await init();
    warmup_js(); // build model tables off the critical path
    wasmReady = true;
    el('solveStatus').textContent = 'Engine ready.';
  } catch (e) {
    console.error(e);
    el('solveStatus').textContent = 'Failed to load WASM engine: ' + e.message;
  }
}

main();
