#!/usr/bin/env python3
"""Render the static daily-answers page from a results JSON file.

Usage: render_answers_page.py <results/YYYY-MM-DD.json> <site-dir>
Writes <site-dir>/answers/index.html — a fully static page (no build step,
unlimited traffic). The archive calendar on the page queries the D1-backed
worker API for past dates; only the "today" content is baked in.

Only clean answers are rendered: entries with status != 'ok' or needs_review
are skipped (same rule as the worker ingest filter).
"""
import html
import json
import os
import sys

WORKER_API = "https://puzzle-answers.avm-studio-video.workers.dev"

TYPE_LABEL = {
    "cryptoquote": "Cryptoquote",
    "cryptoquip": "Cryptoquip",
    "celebrity_cipher": "Celebrity Cipher",
}

TYPE_SOURCE = {
    "cryptoquote": "Arkansas Democrat-Gazette",
    "cryptoquip": "Cecil Daily",
    "celebrity_cipher": "cryptoquip.net",
}


def esc(s):
    return html.escape(s or "", quote=True)


def card(p):
    label = TYPE_LABEL.get(p.get("type"), p.get("type", "?"))
    source = TYPE_SOURCE.get(p.get("type"), "")
    teaser = esc(p.get("teaser") or "")
    answer = esc(p.get("answer") or "")
    clue = esc(p.get("clue") or "")
    clue_html = f'<span class="badge text-bg-info ms-2">Clue: {clue}</span>' if clue else ""
    return f"""
      <div class="card shadow-sm mb-3">
        <div class="card-body">
          <div class="d-flex align-items-center mb-2">
            <h2 class="h5 m-0 me-auto">{esc(label)}</h2>
            <span class="badge text-bg-secondary">{esc(source)}</span>{clue_html}
          </div>
          <p class="text-secondary small mb-1"><em>{teaser}&hellip;</em></p>
          <p class="answer-text mb-0">{answer}</p>
        </div>
      </div>"""


def render(results_path, site_dir):
    with open(results_path) as f:
        data = json.load(f)
    puzzles = data.get("puzzles") or []
    clean = [p for p in puzzles
             if (p.get("status") or "ok") == "ok"
             and not p.get("needs_review") and p.get("answer")]
    day = data.get("date") or (os.path.basename(results_path)[:10])
    cards = "\n".join(card(p) for p in clean) or \
        '<div class="alert alert-warning">No verified answers for this date yet.</div>'

    page = f"""<!DOCTYPE html>
<html lang="en" data-bs-theme="light">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Daily Puzzle Answers — {esc(day)} · Substitution Cipher Solver</title>
  <meta name="description" content="Solved answers for today's Cryptoquote, Cryptoquip and Celebrity Cipher, decoded by our own solver.">
  <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css" rel="stylesheet">
  <link href="https://cdn.jsdelivr.net/npm/bootstrap-icons@1.11.3/font/bootstrap-icons.min.css" rel="stylesheet">
  <link href="../style.css" rel="stylesheet">
  <style>
    .answer-text {{ font-size: 1.15rem; font-weight: 600; line-height: 1.5; }}
    .archive-card {{ cursor: default; }}
  </style>
</head>
<body>
  <nav class="navbar navbar-expand-lg bg-body-tertiary border-bottom sticky-top">
    <div class="container">
      <a class="navbar-brand d-flex align-items-center gap-2" href="../">
        <i class="bi bi-shuffle text-primary"></i>
        <span class="fw-semibold">Substitution Cipher</span>
      </a>
      <div class="d-flex align-items-center gap-2 ms-auto">
        <a class="btn btn-sm btn-outline-primary" href="../">Solver</a>
        <button class="btn btn-sm btn-outline-secondary" id="themeToggle" title="Toggle theme">
          <i class="bi bi-moon-stars"></i>
        </button>
      </div>
    </div>
  </nav>

  <main class="container py-4">
    <header class="mb-4">
      <h1 class="h3">Daily Answers <span class="text-secondary fs-6">· {esc(day)}</span></h1>
      <p class="text-secondary mb-0">
        Every answer below was decoded from the original publisher puzzle by our own
        solver — never copied from answer sites. Only answers that pass all automatic
        checks are published.
      </p>
    </header>

    <section id="today">{cards}</section>

    <section class="mt-5">
      <h2 class="h4 mb-3"><i class="bi bi-calendar3"></i> Answer archive</h2>
      <p class="text-secondary">Pick any past date to look up its answers from the archive.</p>
      <div class="row g-2 align-items-end mb-3" style="max-width: 480px">
        <div class="col">
          <label for="arcDate" class="form-label small">Date</label>
          <input type="date" id="arcDate" class="form-control" max="{esc(day)}">
        </div>
        <div class="col-auto">
          <button class="btn btn-primary" id="arcGo">Look up</button>
        </div>
      </div>
      <div id="arcResult"></div>
    </section>

    <footer class="mt-5 pt-3 border-top text-secondary small">
      Cryptoquote &amp; Cryptoquip &copy; King Features Syndicate ·
      Celebrity Cipher by Luis Campos · Answers computed by the substitution-cipher autosolver.
    </footer>
  </main>

  <script>
    const API = "{WORKER_API}";
    const label = t => ({{cryptoquote:"Cryptoquote",cryptoquip:"Cryptoquip",celebrity_cipher:"Celebrity Cipher"}})[t] || t;
    document.getElementById("arcGo").addEventListener("click", async () => {{
      const d = document.getElementById("arcDate").value;
      const box = document.getElementById("arcResult");
      if (!d) {{ box.innerHTML = '<div class="alert alert-warning">Pick a date first.</div>'; return; }}
      box.innerHTML = '<div class="spinner-border spinner-border-sm"></div> Loading…';
      try {{
        const r = await fetch(API + "/answers?date=" + encodeURIComponent(d));
        if (!r.ok) throw new Error("not found");
        const j = await r.json();
        const ps = (j.puzzles || []).filter(p => (p.status || "ok") === "ok" && !p.needs_review && p.answer);
        if (!ps.length) {{ box.innerHTML = '<div class="alert alert-info">No verified answers archived for ' + d + '.</div>'; return; }}
        box.innerHTML = ps.map(p => `
          <div class="card shadow-sm mb-3 archive-card"><div class="card-body">
            <h3 class="h6">${{label(p.type)}}</h3>
            <p class="answer-text mb-0">${{p.answer.replace(/&/g,"&amp;").replace(/</g,"&lt;")}}</p>
          </div></div>`).join("");
      }} catch (e) {{
        box.innerHTML = '<div class="alert alert-warning">Could not load answers for ' + d + '.</div>';
      }}
    }});
    document.getElementById("themeToggle").addEventListener("click", () => {{
      const h = document.documentElement;
      h.dataset.bsTheme = h.dataset.bsTheme === "light" ? "dark" : "light";
    }});
  </script>
  <script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/js/bootstrap.bundle.min.js"></script>
</body>
</html>
"""
    out_dir = os.path.join(site_dir, "answers")
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "index.html"), "w") as f:
        f.write(page)
    print(f"wrote {out_dir}/index.html ({len(clean)} answers for {day})")


if __name__ == "__main__":
    render(sys.argv[1], sys.argv[2])
