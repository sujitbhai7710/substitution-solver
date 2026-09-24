// puzzle-answers: ingest daily solver results from the GitHub pipeline repo into D1,
// and serve them as a small JSON API.
//
// GitHub Action (scheduled) commits pipeline/results/YYYY-MM-DD.json to the repo.
// This worker (scheduled daily, after the Action) pulls the raw JSON and upserts
// it into D1, then serves:
//   GET /answers?date=YYYY-MM-DD   -> that day's answers
//   GET /answers/latest            -> most recent day with answers
//
// Env vars: GITHUB_OWNER, GITHUB_REPO (the pipeline repo, must be public or the
// raw URLs must be reachable).

async function ingestDay(env, day) {
  const owner = env.GITHUB_OWNER, repo = env.GITHUB_REPO;
  if (!owner || !repo) throw new Error("GITHUB_OWNER/GITHUB_REPO not set");
  const url = `https://raw.githubusercontent.com/${owner}/${repo}/main/pipeline/results/${day}.json`;
  const started = new Date().toISOString();
  let status = "ok", log = "";
  try {
    const res = await fetch(url, { headers: { "User-Agent": "puzzle-answers-worker" } });
    if (!res.ok) throw new Error(`github raw ${res.status} for ${day}`);
    const data = await res.json();
    for (const p of data.puzzles || []) {
      await env.DB.prepare(
        `INSERT INTO answers (day, type, source, teaser, clue, answer, attribution,
          solver_key, solver_score, solver_ms, ocr_conf, checks_json, needs_review,
          status, generated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(day, type) DO UPDATE SET
          source=excluded.source, teaser=excluded.teaser, clue=excluded.clue,
          answer=excluded.answer, attribution=excluded.attribution,
          solver_key=excluded.solver_key, solver_score=excluded.solver_score,
          solver_ms=excluded.solver_ms, ocr_conf=excluded.ocr_conf,
          checks_json=excluded.checks_json, needs_review=excluded.needs_review,
          status=excluded.status, generated_at=excluded.generated_at,
          ingested_at=datetime('now')`
      ).bind(
        p.date || day, p.type, p.source, p.teaser || null, p.clue || null,
        p.answer || null, p.attribution || null,
        (p.solver && p.solver.key) || null, (p.solver && p.solver.score) || null,
        (p.solver && p.solver.ms) || null, p.ocr_conf ?? null,
        p.checks ? JSON.stringify(p.checks) : null,
        p.needs_review ? 1 : 0, p.status || "ok", data.generated_at || null
      ).run();
    }
    log = `ingested ${(data.puzzles || []).length} puzzles`;
  } catch (e) {
    status = "failed";
    log = String((e && e.message) || e).slice(0, 500);
  }
  await env.DB.prepare(
    `INSERT INTO pipeline_runs (day, started_at, finished_at, status, log)
     VALUES (?, ?, ?, ?, ?)
     ON CONFLICT(day) DO UPDATE SET finished_at=excluded.finished_at,
       status=excluded.status, log=excluded.log`
  ).bind(day, started, new Date().toISOString(), status, log).run();
  return { day, status, log };
}

function json(data, code = 200) {
  return new Response(JSON.stringify(data), {
    status: code,
    headers: { "Content-Type": "application/json", "Access-Control-Allow-Origin": "*" },
  });
}

export default {
  async fetch(req, env) {
    const url = new URL(req.url);
    if (url.pathname === "/answers") {
      const day = url.searchParams.get("date");
      if (!day) return json({ error: "date required (YYYY-MM-DD)" }, 400);
      const rows = await env.DB.prepare(
        "SELECT day, type, source, teaser, clue, answer, attribution, ocr_conf, needs_review, status FROM answers WHERE day = ?"
      ).bind(day).all();
      return json({ date: day, puzzles: rows.results });
    }
    if (url.pathname === "/answers/latest") {
      const d = await env.DB.prepare(
        "SELECT day FROM answers ORDER BY day DESC LIMIT 1"
      ).first();
      if (!d) return json({ error: "no answers yet" }, 404);
      const rows = await env.DB.prepare(
        "SELECT day, type, source, teaser, clue, answer, attribution, ocr_conf, needs_review, status FROM answers WHERE day = ?"
      ).bind(d.day).all();
      return json({ date: d.day, puzzles: rows.results });
    }
    if (url.pathname === "/ingest") {
      // manual trigger: /ingest?date=YYYY-MM-DD
      const day = url.searchParams.get("date") || new Date().toISOString().slice(0, 10);
      return json(await ingestDay(env, day));
    }
    return new Response("puzzle-answers ok");
  },
  async scheduled(event, env, ctx) {
    // Runs daily after the GitHub Action: ingest today (+ yesterday as catch-up).
    ctx.waitUntil((async () => {
      const today = new Date().toISOString().slice(0, 10);
      const y = new Date(Date.now() - 864e5).toISOString().slice(0, 10);
      await ingestDay(env, y);
      await ingestDay(env, today);
    })());
  },
};
