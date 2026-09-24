// Decipher frontend — Substitution Cipher Solver.
// CRITICAL: the ciphertext is passed to WASM byte-for-byte unchanged.
// No normalization of curly quotes, dashes, casing, or whitespace happens
// here — the Rust engine preserves everything verbatim on output.

import init, { solve_n_js, warmup_js } from './pkg/subst_solver.js';

let wasmReady = false;
let selectedCard = null;

const el = (id) => document.getElementById(id);

function countLetters(s) {
  return (s.match(/[A-Za-z]/g) || []).length;
}

function updateStats() {
  const v = el('toolInput').value;
  el('inputStats').innerHTML =
    `<i class="bi bi-type"></i> ${v.length} chars · ${countLetters(v)} letters`;
}

function setBusy(state, msg) {
  el('solveSpinner').classList.toggle('d-none', !state);
  el('autosolveButton').disabled = state;
  if (msg !== undefined) el('solveStatus').textContent = msg;
}

function showError(msg) {
  el('errorMsg').textContent = msg;
  el('errorCard').classList.remove('d-none');
  el('errorMsg').scrollIntoView({ behavior: 'smooth', block: 'nearest' });
}

function clearError() {
  el('errorCard').classList.add('d-none');
}

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

// Render the substitution key as cipher→plain pairs, e.g. "A→E B→T".
function renderKey(key) {
  let entries = [];
  if (typeof key === 'string' && key.length === 26) {
    // Plain alphabet aligned to cipher A–Z: key[i] is plain for cipher A+i.
    entries = Array.from(key).map((p, i) => [String.fromCharCode(65 + i), p]);
  } else if (typeof key === 'string') {
    return `<span class="key-pair"><span class="key-plain">${escapeHtml(key)}</span></span>`;
  } else if (Array.isArray(key)) {
    entries = key.filter((p) => p && p.length >= 2).map((p) => [String(p[0]), String(p[1])]);
  } else if (key && typeof key === 'object') {
    entries = Object.keys(key).sort().map((c) => [c, String(key[c])]);
  }
  return entries
    .map(([c, p]) => `<span class="key-pair"><span class="key-cipher">${escapeHtml(c)}</span><span class="key-arrow">→</span><span class="key-plain">${escapeHtml(p)}</span></span>`)
    .join('');
}

function renderResults(results, ms) {
  const list = el('resultsList');
  list.innerHTML = '';
  selectedCard = null;
  // Collapse exact duplicate plaintexts (engine may repeat a candidate).
  const seen = new Set();
  const unique = results.filter((r) => {
    if (seen.has(r.plaintext)) return false;
    seen.add(r.plaintext);
    return true;
  });
  results = unique;
  const bestScore = results.length ? results[0].score : 0;

  results.forEach((r, i) => {
    const card = document.createElement('article');
    card.className = 'result-card' + (i === 0 ? ' rank-1' : '');
    card.tabIndex = 0;
    const pct = bestScore > 0 ? Math.max(4, Math.round((r.score / bestScore) * 100)) : 4;
    card.innerHTML = `
      <div class="result-head">
        <span class="rank-badge">#${i + 1}</span>
        ${i === 0 ? '<span class="best-pill"><i class="bi bi-award"></i> Best</span>' : ''}
        <span class="score-badge ms-auto" title="Combined final-selection score"><i class="bi bi-speedometer2"></i> ${r.score.toFixed(1)}</span>
        <button class="btn btn-sm btn-copy" data-copy="${i}" title="Copy plaintext">
          <i class="bi bi-clipboard"></i> Copy
        </button>
      </div>
      <div class="score-bar"><span style="width:${pct}%"></span></div>
      <pre class="result-body">${escapeHtml(r.plaintext)}</pre>
      <div class="result-key"><span class="key-label">Key</span> ${renderKey(r.key)}</div>
    `;
    card.addEventListener('click', (e) => {
      if (e.target.closest('[data-copy]')) return;
      if (selectedCard) selectedCard.classList.remove('selected');
      selectedCard = card;
      card.classList.add('selected');
    });
    list.appendChild(card);
  });

  list.querySelectorAll('[data-copy]').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const text = results[parseInt(btn.dataset.copy, 10)].plaintext;
      try {
        await navigator.clipboard.writeText(text);
        const original = btn.innerHTML;
        btn.innerHTML = '<i class="bi bi-check2"></i> Copied';
        btn.classList.add('copied');
        setTimeout(() => { btn.innerHTML = original; btn.classList.remove('copied'); }, 1400);
      } catch (err) {
        showError('Clipboard copy failed: ' + err.message);
      }
    });
  });

  el('resultsSection').classList.remove('d-none');
  el('solveMeta').textContent =
    `Engine ${results[0] && results[0].ms !== undefined ? results[0].ms.toFixed(0) + ' ms · ' : ''}score ${results[0] ? results[0].score.toFixed(1) : '—'}`;
  return results; // deduped, in case the engine repeated a candidate
}

async function runSolver() {
  // Pass the raw textarea value straight through — no cleanup, no
  // case-folding, no punctuation normalization. The engine handles it.
  const puzzle = el('toolInput').value;
  if (countLetters(puzzle) < 4) {
    showError('Please provide at least 4 letters of ciphertext.');
    return;
  }
  if (!wasmReady) {
    showError('Engine still loading — try again in a moment.');
    return;
  }
  clearError();
  const clue = el('clueInput').value;
  const depth = parseInt(el('searchDepth').value, 10) || 6000;
  const quality = el('analysisQuality').value;
  const restarts = quality === 'high' ? 192 : 24;
  const steps = quality === 'high' ? Math.min(depth, 6000) : Math.min(depth, 6000);
  const n = parseInt(el('resultsCount').value, 10) || 5;

  setBusy(true, `Solving — returning top ${n} candidate${n > 1 ? 's' : ''}…`);
  // Let the browser paint before the synchronous WASM call blocks.
  await new Promise((r) => setTimeout(r, 30));

  const t0 = performance.now();
  let results;
  try {
    // solve_n_js(puzzle, clue, restarts, steps, n) -> JSON array of
    // {"plaintext","key","score"}, best first.
    results = JSON.parse(solve_n_js(puzzle, clue, restarts, steps, n));
  } catch (e) {
    console.error(e);
    setBusy(false, 'Solver error.');
    showError('Solver error: ' + e.message);
    return;
  }
  const dt = performance.now() - t0;

  if (!Array.isArray(results) || results.length === 0) {
    setBusy(false, 'No candidates found.');
    showError('The solver produced no candidates for this input.');
    return;
  }

  const rendered = renderResults(results, dt);
  el('solveMeta').textContent =
    `Solved in ${dt.toFixed(0)} ms · showing ${rendered.length} candidate${rendered.length > 1 ? 's' : ''}`;
  el('footerStats').textContent = `Last solve: ${dt.toFixed(0)} ms`;
  setBusy(false, 'Done.');
}

// Theme toggle (persisted)
function initTheme() {
  const html = document.documentElement;
  const stored = localStorage.getItem('decipher-theme');
  if (stored) html.setAttribute('data-bs-theme', stored);
  el('themeToggle').addEventListener('click', () => {
    const cur = html.getAttribute('data-bs-theme');
    const next = cur === 'dark' ? 'light' : 'dark';
    html.setAttribute('data-bs-theme', next);
    localStorage.setItem('decipher-theme', next);
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
      showError('Clipboard read failed: ' + e.message);
    }
  });
  updateStats();

  try {
    await init();
    warmup_js(); // build model tables off the critical path
    wasmReady = true;
    el('solveStatus').textContent = 'Engine ready — hit Solve.';
  } catch (e) {
    console.error(e);
    el('solveStatus').textContent = 'Failed to load WASM engine: ' + e.message;
  }
}

main();
