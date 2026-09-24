// Substitution Cipher autosolver — front-end controller.
// Loads the Rust/WASM engine and wires up the manual + autosolver UI.

import init, { solve_ex, solve_flat, init_models } from './pkg/subst_solver.js';

const { Tab } = window.bootstrap;

const ALPHA = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ';

const el = (id) => document.getElementById(id);

let wasmReady = false;
let busy = false;

// ---------------------------------------------------------------------------
// WASM bootstrap
// ---------------------------------------------------------------------------
async function boot() {
  const badge = el('engineBadge');
  badge.textContent = 'Loading engine…';
  try {
    await init();
    // Load the n-gram + dictionary data blobs and hand them to the engine.
    const [quad, wf] = await Promise.all([
      fetch('./data/quadgrams.bin').then((r) => r.arrayBuffer()),
      fetch('./data/wordfreq.bin').then((r) => r.arrayBuffer()),
    ]);
    init_models(new Uint8Array(quad), new Uint8Array(wf));
    wasmReady = true;
    badge.textContent = 'Rust · WASM ready';
    badge.className = 'badge text-bg-success';
  } catch (e) {
    console.error('WASM init failed', e);
    badge.textContent = 'Engine failed';
    badge.className = 'badge text-bg-danger';
  }
}

// ---------------------------------------------------------------------------
// Text helpers
// ---------------------------------------------------------------------------
function normalizeText(s) {
  // Uppercase letters; collapse all non-letters to single spaces (like the solver).
  let out = '';
  let prevSpace = false;
  for (const ch of s.toUpperCase()) {
    if (ch >= 'A' && ch <= 'Z') {
      out += ch;
      prevSpace = false;
    } else if (!prevSpace && out.length > 0) {
      out += ' ';
      prevSpace = true;
    }
  }
  return out.trim();
}

// Raw text keeps every punctuation mark (apostrophes, quotes, dashes, sentence
// stops) — the engine uses them as constraints and restores them in the output.
function rawText(s) {
  return s
    .replace(/[\u2018\u2019]/g, "'")
    .replace(/[\u201C\u201D\u201E]/g, '"')
    .replace(/[\u2013\u2014]/g, '-');
}

function lettersOnly(s) {
  return s.replace(/[^A-Za-z]/g, '').toUpperCase();
}

function countLetters(s) {
  return (s.match(/[A-Za-z]/g) || []).length;
}

// ---------------------------------------------------------------------------
// Manual tab: key grid + live decrypt
// ---------------------------------------------------------------------------
let key = {}; // cipherLetter -> plainLetter

function buildKeyGrid() {
  const grid = el('keyGrid');
  grid.innerHTML = '';
  for (const c of ALPHA) {
    const cell = document.createElement('div');
    cell.className = 'key-cell unassigned';
    cell.innerHTML = `
      <span class="cipher-letter">${c}</span>
      <span class="arrow">&rarr;</span>
      <input type="text" maxlength="1" data-cipher="${c}" value="" aria-label="${c} maps to">
    `;
    grid.appendChild(cell);
    const input = cell.querySelector('input');
    input.addEventListener('input', () => {
      let v = input.value.toUpperCase().replace(/[^A-Z]/g, '');
      input.value = v;
      if (v) key[c] = v; else delete key[c];
      cell.classList.toggle('unassigned', !v);
      applyManual();
    });
  }
}

function setKeyFromString(keyStr) {
  // keyStr: plain->cipher; we need cipher->plain
  const decrypt = {};
  for (let i = 0; i < 26; i++) {
    decrypt[keyStr[i]] = ALPHA[i];
  }
  key = decrypt;
  syncKeyGrid();
  applyManual();
}

function syncKeyGrid() {
  document.querySelectorAll('#keyGrid input').forEach((input) => {
    const c = input.dataset.cipher;
    input.value = key[c] || '';
    input.closest('.key-cell').classList.toggle('unassigned', !key[c]);
  });
}

function applyManual() {
  const raw = el('toolInput').value;
  const out = decryptText(raw, key);
  el('manualOutput').value = out;
}

function decryptText(text, keyMap) {
  let out = '';
  for (const ch of text) {
    const up = ch.toUpperCase();
    if (up >= 'A' && up <= 'Z') {
      out += keyMap[up] || '·';
    } else {
      out += ch; // spaces and punctuation pass through untouched
    }
  }
  return out;
}

// ---------------------------------------------------------------------------
// Autosolver
// ---------------------------------------------------------------------------
function setBusy(state, msg) {
  busy = state;
  el('solveSpinner').classList.toggle('d-none', !state);
  el('autosolveButton').disabled = state;
  el('stopButton').classList.toggle('d-none', !state);
  if (msg !== undefined) el('solveStatus').textContent = msg;
}

function qualityToSteps(q, depth) {
  // depth is the user's "search depth"; quality scales restarts/steps
  const base = parseInt(depth, 10) || 12000;
  if (q === 'high') return { rounds: 80, steps: Math.min(base * 2, 200000) };
  return { rounds: 40, steps: base };
}

async function runAutosolver() {
  if (busy) return;
  const raw = el('toolInput').value;
  const text = rawText(raw);
  if (countLetters(text) < 4) {
    el('solveStatus').textContent = 'Please provide at least 4 letters.';
    return;
  }
  if (!wasmReady) {
    el('solveStatus').textContent = 'Engine still loading, try again in a moment.';
    return;
  }

  const crib = el('expectedText').value.trim();
  const keepBreaks = el('delimiterKeep').checked;
  const depth = el('searchDepth').value;
  const quality = el('analysisQuality').value;
  const keep = Math.max(1, Math.min(20, parseInt(el('numberOfResults').value, 10) || 5));
  const { rounds, steps } = qualityToSteps(quality, depth);

  setBusy(true, 'Searching…');
  // let the browser paint the spinner before we block on compute
  await new Promise((r) => setTimeout(r, 20));

  const t0 = performance.now();
  let results;
  try {
    results = keepBreaks
      ? JSON.parse(solve_ex(text, crib, rounds, steps, 12.0, 8.0)).results
      : JSON.parse(solve_flat(text, crib, rounds, steps)).results;
  } catch (e) {
    console.error(e);
    setBusy(false, 'Solver error: ' + e.message);
    return;
  }
  const dt = performance.now() - t0;

  renderResults(results.slice(0, keep), crib);
  setBusy(false, `Done in ${(dt / 1000).toFixed(2)} s · ${dt.toFixed(0)} ms · ${results.length} candidates.`);
  el('footerStats').textContent = `Last solve: ${dt.toFixed(0)} ms`;
}

function renderResults(results, crib) {
  const c = el('resultsContainer');
  c.innerHTML = '';
  if (!results.length) {
    c.innerHTML = '<p class="text-secondary">No results.</p>';
    return;
  }
  results.forEach((r, i) => {
    const card = document.createElement('div');
    card.className = 'result-card' + (i === 0 ? ' rank-1' : '');
    const head = document.createElement('div');
    head.className = 'result-head';
    head.innerHTML = `
      <span class="badge text-bg-${i === 0 ? 'primary' : 'secondary'}">#${i + 1}</span>
      <span class="text-secondary score-badge">score ${r.total_score.toFixed(1)}</span>
      <span class="text-secondary score-badge">ngram ${r.ngram_score.toFixed(0)}</span>
      <span class="text-secondary score-badge">word ${r.word_score.toFixed(0)}</span>
      <span class="ms-auto"></span>
    `;
    const useBtn = document.createElement('button');
    useBtn.className = 'btn btn-sm btn-outline-primary';
    useBtn.innerHTML = '<i class="bi bi-key"></i> Use key';
    useBtn.onclick = () => {
      setKeyFromString(r.key);
      Tab.getOrCreateInstance(el('manual-tab')).show();
    };
    const copyBtn = document.createElement('button');
    copyBtn.className = 'btn btn-sm btn-outline-secondary';
    copyBtn.innerHTML = '<i class="bi bi-clipboard"></i>';
    copyBtn.title = 'Copy plaintext';
    copyBtn.onclick = () => navigator.clipboard.writeText(r.plaintext);
    head.appendChild(useBtn);
    head.appendChild(copyBtn);

    const body = document.createElement('div');
    body.className = 'result-body';
    body.textContent = r.plaintext; // punctuation preserved by the engine

    card.appendChild(head);
    card.appendChild(body);
    c.appendChild(card);
  });
}

// ---------------------------------------------------------------------------
// Small UI actions
// ---------------------------------------------------------------------------
async function copyFrom(elId, btn) {
  try {
    await navigator.clipboard.writeText(el(elId).value);
    const old = btn.innerHTML;
    btn.innerHTML = '<i class="bi bi-check2"></i> Copied';
    setTimeout(() => (btn.innerHTML = old), 1200);
  } catch {}
}

async function pasteInto(elId) {
  try {
    const t = await navigator.clipboard.readText();
    el(elId).value = t;
    el(elId).dispatchEvent(new Event('input'));
  } catch {}
}

function updateStats() {
  const v = el('toolInput').value;
  el('inputStats').textContent = `${v.length} chars · ${countLetters(v)} letters`;
}

// ---------------------------------------------------------------------------
// Wire up
// ---------------------------------------------------------------------------
function main() {
  buildKeyGrid();

  el('toolInput').addEventListener('input', () => {
    updateStats();
    applyManual();
  });
  el('copyInputBtn').onclick = (e) => copyFrom('toolInput', e.currentTarget);
  el('pasteInputBtn').onclick = () => pasteInto('toolInput');
  el('clearInputBtn').onclick = () => {
    el('toolInput').value = '';
    updateStats();
    applyManual();
  };
  el('copyOutputBtn').onclick = (e) => copyFrom('manualOutput', e.currentTarget);

  el('clearKeyButton').onclick = () => { key = {}; syncKeyGrid(); applyManual(); };
  el('randomizeKeyButton').onclick = () => {
    const perm = ALPHA.split('');
    for (let i = perm.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [perm[i], perm[j]] = [perm[j], perm[i]];
    }
    key = {};
    ALPHA.split('').forEach((c, i) => (key[c] = perm[i]));
    syncKeyGrid();
    applyManual();
  };
  el('invertKeyButton').onclick = () => {
    const inv = {};
    for (const [c, p] of Object.entries(key)) inv[p] = c;
    key = inv;
    syncKeyGrid();
    applyManual();
  };

  el('autosolveButton').onclick = runAutosolver;
  el('stopButton').onclick = () => setBusy(false, 'Stopped.');

  el('themeToggle').onclick = () => {
    const cur = document.documentElement.getAttribute('data-bs-theme');
    document.documentElement.setAttribute('data-bs-theme', cur === 'dark' ? 'light' : 'dark');
  };

  updateStats();
  boot();
}

main();
