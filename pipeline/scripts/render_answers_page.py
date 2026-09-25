#!/usr/bin/env python3
"""Render the static daily-answers site from a results JSON file.

Usage: render_answers_page.py <results/YYYY-MM-DD.json> <site-dir>

Writes:
  <site-dir>/answers/index.html                  — hub, links to the three types
  <site-dir>/answers/cryptoquote/index.html      — today's Cryptoquote answer
  <site-dir>/answers/cryptoquip/index.html       — today's Cryptoquip answer
  <site-dir>/answers/celebrity-cipher/index.html — today's Celebrity Cipher answer

Each type page shows the FULL ciphertext plus the decoded answer (both
produced by our own pipeline — never copied from answer sites). The archive
lookup on each page queries the D1-backed worker API for past dates,
filtered to that puzzle type. Only clean answers are rendered: entries with
status != 'ok' or needs_review are skipped (same rule as worker ingest).
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

TYPE_SLUG = {
    "cryptoquote": "cryptoquote",
    "cryptoquip": "cryptoquip",
    "celebrity_cipher": "celebrity-cipher",
}

TYPE_SOURCE = {
    "cryptoquote": "Arkansas Democrat-Gazette",
    "cryptoquip": "Cecil Daily",
    "celebrity_cipher": "cryptoquip.net",
}

TYPE_BLURB = {
    "cryptoquote": "Cryptoquote &copy; King Features Syndicate, via Arkansas Democrat-Gazette.",
    "cryptoquip": "Cryptoquip &copy; King Features Syndicate, via Cecil Daily.",
    "celebrity_cipher": "Celebrity Cipher by Luis Campos, via cryptoquip.net.",
}

PAGE_HEAD = """<!DOCTYPE html>
<html lang="en" data-bs-theme="light">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
  <meta name="description" content="{desc}">
  <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css" rel="stylesheet">
  <link href="https://cdn.jsdelivr.net/npm/bootstrap-icons@1.11.3/font/bootstrap-icons.min.css" rel="stylesheet">
  <link href="{css}" rel="stylesheet">
  <style>
    .answer-text {{ font-size: 1.2rem; font-weight: 600; line-height: 1.55; }}
    .cipher-text {{ font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
                    font-size: 1.02rem; line-height: 1.7; letter-spacing: .02em; }}
  </style>
</head>
<body>
  <nav class="navbar navbar-expand-lg bg-body-tertiary border-bottom sticky-top">
    <div class="container">
      <a class="navbar-brand d-flex align-items-center gap-2" href="{home}">
        <i class="bi bi-shuffle text-primary"></i>
        <span class="fw-semibold">Substitution Cipher</span>
      </a>
      <div class="d-flex align-items-center gap-2 ms-auto">
        <a class="btn btn-sm btn-outline-primary" href="{home}">Solver</a>
        <a class="btn btn-sm btn-outline-secondary" href="{answers}">All answers</a>
        <button class="btn btn-sm btn-outline-secondary" id="themeToggle" title="Toggle theme">
          <i class="bi bi-moon-stars"></i>
        </button>
      </div>
    </div>
  </nav>
  <main class="container py-4">
"""

PAGE_FOOT = """
    <footer class="mt-5 pt-3 border-top text-secondary small">
      {credit} Answers computed by the substitution-cipher autosolver.
    </footer>
  </main>
  <script>
    document.getElementById("themeToggle").addEventListener("click", () => {{
      const h = document.documentElement;
      h.dataset.bsTheme = h.dataset.bsTheme === "light" ? "dark" : "light";
    }});
  </script>
  <script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/js/bootstrap.bundle.min.js"></script>
</body>
</html>
"""

ARCHIVE_JS_TMPL = """
    const API = "%s";
    const TYPE = "%s";
    document.getElementById("arcGo").addEventListener("click", async () => {
      const d = document.getElementById("arcDate").value;
      const box = document.getElementById("arcResult");
      if (!d) { box.innerHTML = '<div class="alert alert-warning">Pick a date first.</div>'; return; }
      box.innerHTML = '<div class="spinner-border spinner-border-sm"></div> Loading…';
      try {
        const r = await fetch(API + "/answers?date=" + encodeURIComponent(d) + "&type=" + TYPE);
        if (!r.ok) throw new Error("not found");
        const j = await r.json();
        const ps = (j.puzzles || []).filter(p => (p.status || "ok") === "ok" && !p.needs_review && p.answer);
        if (!ps.length) { box.innerHTML = '<div class="alert alert-info">No verified answer archived for ' + d + '.</div>'; return; }
        const esc = s => (s || "").replace(/&/g,"&amp;").replace(/</g,"&lt;");
        box.innerHTML = ps.map(p => `
          <div class="card shadow-sm mb-3"><div class="card-body">
            <p class="cipher-text text-secondary mb-2">${esc(p.ciphertext)}</p>
            <p class="answer-text mb-0">${esc(p.answer)}</p>
          </div></div>`).join("");
      } catch (e) {
        box.innerHTML = '<div class="alert alert-warning">Could not load the answer for ' + d + '.</div>';
      }
    });
"""


def esc(s):
    return html.escape(s or "", quote=True)


def answer_card(p):
    """Full ciphertext + answer card for a type page."""
    label = TYPE_LABEL.get(p.get("type"), p.get("type", "?"))
    source = TYPE_SOURCE.get(p.get("type"), "")
    cipher = esc(p.get("ciphertext") or "")
    answer = esc(p.get("answer") or "")
    clue = esc(p.get("clue") or "")
    clue_html = (f'<span class="badge text-bg-info ms-2">Clue: {clue}</span>'
                 if clue else "")
    return f"""
      <div class="card shadow-sm mb-3">
        <div class="card-body">
          <div class="d-flex align-items-center mb-3">
            <h2 class="h5 m-0 me-auto">{esc(label)}</h2>
            <span class="badge text-bg-secondary">{esc(source)}</span>{clue_html}
          </div>
          <p class="text-secondary small mb-1"><i class="bi bi-lock"></i> Encrypted</p>
          <p class="cipher-text mb-3">{cipher}</p>
          <p class="text-secondary small mb-1"><i class="bi bi-unlock"></i> Decoded by our solver</p>
          <p class="answer-text mb-0">{answer}</p>
        </div>
      </div>"""


def archive_section(day, ptype):
    js = ARCHIVE_JS_TMPL % (WORKER_API, ptype)
    return f"""
    <section class="mt-5">
      <h2 class="h4 mb-3"><i class="bi bi-calendar3"></i> Answer archive</h2>
      <p class="text-secondary">Pick any past date to look up its {esc(TYPE_LABEL[ptype])} answer from the archive.</p>
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
    <script>{js}</script>"""


def type_page(p, day):
    ptype = p["type"]
    label = TYPE_LABEL[ptype]
    head = PAGE_HEAD.format(
        title=f"Today's {label} Answer — {day} · Substitution Cipher Solver",
        desc=f"Today's {label} decoded by our own solver. Full ciphertext plus answer.",
        css="../../style.css", home="../../", answers="../")
    body = f"""
    <header class="mb-4">
      <nav aria-label="breadcrumb">
        <ol class="breadcrumb">
          <li class="breadcrumb-item"><a href="../">Daily Answers</a></li>
          <li class="breadcrumb-item active">{esc(label)}</li>
        </ol>
      </nav>
      <h1 class="h3">Today's {esc(label)} Answer <span class="text-secondary fs-6">· {esc(day)}</span></h1>
      <p class="text-secondary mb-0">
        Decoded from the original publisher puzzle by our own solver — never
        copied from answer sites. Only answers that pass all automatic checks
        are published.
      </p>
    </header>
    <section id="today">{answer_card(p)}</section>
    {archive_section(day, ptype)}
"""
    return head + body + PAGE_FOOT.format(credit=TYPE_BLURB[ptype])


def hub_page(by_type, day):
    head = PAGE_HEAD.format(
        title=f"Daily Puzzle Answers — {day} · Substitution Cipher Solver",
        desc="Solved answers for today's Cryptoquote, Cryptoquip and Celebrity Cipher, decoded by our own solver.",
        css="../style.css", home="../", answers="./")
    links = []
    for ptype in ["cryptoquote", "cryptoquip", "celebrity_cipher"]:
        slug = TYPE_SLUG[ptype]
        label = TYPE_LABEL[ptype]
        p = by_type.get(ptype)
        if p and p.get("answer"):
            teaser = esc((p.get("answer") or "")[:90])
            links.append(f"""
        <a href="./{slug}/" class="list-group-item list-group-item-action">
          <div class="d-flex w-100 justify-content-between align-items-center">
            <h2 class="h5 mb-1">{esc(label)}</h2>
            <span class="badge text-bg-success">answered</span>
          </div>
          <p class="mb-1 text-secondary small">{teaser}&hellip;</p>
          <small class="text-primary">See full ciphertext + answer <i class="bi bi-arrow-right"></i></small>
        </a>""")
        else:
            links.append(f"""
        <div class="list-group-item">
          <div class="d-flex w-100 justify-content-between align-items-center">
            <h2 class="h5 mb-1">{esc(label)}</h2>
            <span class="badge text-bg-warning">pending checks</span>
          </div>
          <p class="mb-1 text-secondary small">Today's puzzle is still being verified.</p>
        </div>""")
    body = f"""
    <header class="mb-4">
      <h1 class="h3">Daily Answers <span class="text-secondary fs-6">· {esc(day)}</span></h1>
      <p class="text-secondary mb-0">
        Every answer below was decoded from the original publisher puzzle by our own
        solver — never copied from answer sites. Only answers that pass all automatic
        checks are published.
      </p>
    </header>
    <div class="list-group shadow-sm">{"".join(links)}</div>
    <section class="mt-5">
      <h2 class="h4 mb-3"><i class="bi bi-calendar3"></i> Answer archive</h2>
      <p class="text-secondary">Each puzzle has its own archive — pick a type above, then any past date.</p>
    </section>
"""
    credit = ("Cryptoquote &amp; Cryptoquip &copy; King Features Syndicate · "
              "Celebrity Cipher by Luis Campos ·")
    return head + body + PAGE_FOOT.format(credit=credit)


def render(results_path, site_dir):
    with open(results_path) as f:
        data = json.load(f)
    puzzles = data.get("puzzles") or []
    clean = [p for p in puzzles
             if (p.get("status") or "ok") == "ok"
             and not p.get("needs_review") and p.get("answer")]
    day = data.get("date") or os.path.basename(results_path)[:10]
    by_type = {p["type"]: p for p in clean}

    base = os.path.join(site_dir, "answers")
    # hub
    os.makedirs(base, exist_ok=True)
    with open(os.path.join(base, "index.html"), "w") as f:
        f.write(hub_page(by_type, day))
    # per-type pages
    for ptype, p in by_type.items():
        d = os.path.join(base, TYPE_SLUG[ptype])
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "index.html"), "w") as f:
            f.write(type_page(p, day))
    print(f"rendered answers for {day}: {sorted(by_type)}")


if __name__ == "__main__":
    render(sys.argv[1], sys.argv[2])
