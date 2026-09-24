const SOURCES = {
  // User-provided publisher pages: Cryptoquip PDFs + Cryptoquote image
  cecil_cryptoquip: "https://www.cecildaily.com/diversions/cryptoquip/",
  ar_cryptoquote: "https://www.arkansasonline.com/puzzles/quote/",
};

const UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

// Try to pull the daily puzzle (ciphertext + clue + date) out of raw HTML.
function extractPuzzle(html) {
  const found = { ciphertexts: [], clues: [], dates: [] };

  // 1. JSON blobs in script tags (Next.js __NEXT_DATA__, ld+json, etc.)
  const scripts = [];
  const re = /<script[^>]*>([\s\S]*?)<\/script>/gi;
  let m;
  while ((m = re.exec(html)) && scripts.length < 60) scripts.push(m[1]);
  for (const s of scripts) {
    // long uppercase runs look like ciphertext
    const cands = s.match(/[A-Z][A-Z' .,\-;?!]{40,}/g) || [];
    for (const c of cands) {
      const t = c.replace(/\\n/g, " ").replace(/\s+/g, " ").trim();
      if (t.length > 40 && t.length < 600 && !found.ciphertexts.includes(t))
        found.ciphertexts.push(t.slice(0, 400));
      if (found.ciphertexts.length >= 8) break;
    }
    // clue patterns like "T=W" or "Clue: T = W"
    const clues = s.match(/\b[A-Z]\s*=\s*[A-Z]\b/g) || [];
    for (const c of clues)
      if (!found.clues.includes(c)) found.clues.push(c);
    // ISO dates near "puzzle"
    const dates = s.match(/20\d\d-[01]\d-[0-3]\d/g) || [];
    for (const d of dates)
      if (!found.dates.includes(d)) found.dates.push(d);
    if (found.ciphertexts.length >= 8) break;
  }

  // 2. Fallback: visible text uppercase runs
  if (found.ciphertexts.length === 0) {
    const text = html
      .replace(/<script[\s\S]*?<\/script>/gi, " ")
      .replace(/<style[\s\S]*?<\/style>/gi, " ")
      .replace(/<[^>]+>/g, " ");
    const cands = text.match(/[A-Z][A-Z' .,\-;?!]{60,}/g) || [];
    for (const c of cands.slice(0, 8)) {
      const t = c.replace(/\s+/g, " ").trim();
      if (t.length > 60 && !found.ciphertexts.includes(t))
        found.ciphertexts.push(t.slice(0, 400));
    }
  }
  // 3. Asset links: PDFs and images that may carry the puzzle
  const linkRe = /(?:href|src)="([^"]+)"/gi;
  const pdfs = [], imgs = [], arts = [];
  while ((m = linkRe.exec(html))) {
    const u = m[1];
    if (/\.pdf(\?|$)/i.test(u) && !pdfs.includes(u) && pdfs.length < 15) pdfs.push(u.slice(0, 220));
    else if (/\.(?:jpe?g|png|webp)(\?|$)/i.test(u) && !imgs.includes(u) && imgs.length < 15) imgs.push(u.slice(0, 220));
    else if (/cryptoqu/i.test(u) && !arts.includes(u) && arts.length < 15 && !u.startsWith("#")) arts.push(u.slice(0, 220));
  }
  found.pdfs = pdfs; found.imgs = imgs; found.articles = arts;
  return found;
}

async function scrapeOne(key, url) {
  const t0 = Date.now();
  try {
    const res = await fetch(url, {
      headers: { "User-Agent": UA, Accept: "text/html" },
      redirect: "follow",
    });
    const text = await res.text();
    const out = {
      source: key,
      url,
      final_url: res.url,
      status: res.status,
      bytes: text.length,
      ms: Date.now() - t0,
    };
    if (res.status === 200) {
      out.extracted = extractPuzzle(text);
      out.html_head = text.slice(0, 60000);
    } else out.snippet = text.slice(0, 200).replace(/\s+/g, " ");
    return out;
  } catch (e) {
    return { source: key, url, error: String((e && e.message) || e), ms: Date.now() - t0 };
  }
}

async function scrape(env, keys, save) {
  const out = {};
  for (const key of keys) {
    const r = await scrapeOne(key, SOURCES[key]);
    out[key] = r;
    if (save && env.DB) {
      const day = new Date().toISOString().slice(0, 10);
      const note = r.status === 200
        ? JSON.stringify(r.extracted || {}).slice(0, 500)
        : ("ERR " + (r.error || ("http " + r.status)) + " :: " + (r.snippet || "")).slice(0, 500);
      await env.DB.prepare(
        "INSERT INTO daily (day, source, url, status, bytes, snippet) VALUES (?, ?, ?, ?, ?, ?) " +
        "ON CONFLICT(day, source) DO UPDATE SET status=excluded.status, bytes=excluded.bytes, snippet=excluded.snippet, fetched_at=datetime('now')"
      )
        .bind(day, key, SOURCES[key], r.status || 0, r.bytes || 0, note)
        .run();
      // Debug: stash raw HTML head for structural inspection (temporary).
      if (r.status === 200 && r.html_head) {
        await env.DB.prepare(
          "INSERT INTO html_debug (source, html, fetched_at) VALUES (?, ?, datetime('now')) " +
          "ON CONFLICT(source) DO UPDATE SET html=excluded.html, fetched_at=datetime('now')"
        ).bind(key, r.html_head).run();
      }
    }
  }
  return out;
}

export default {
  async fetch(req, env) {
    const url = new URL(req.url);
    const one = url.searchParams.get("source");
    const keys = one && SOURCES[one] ? [one] : Object.keys(SOURCES);
    if (url.pathname === "/test-scrape") {
      return Response.json(await scrape(env, keys, false));
    }
    if (url.pathname === "/test-save") {
      const res = await scrape(env, keys, true);
      const rows = env.DB
        ? await env.DB.prepare("SELECT day, source, status, bytes FROM daily ORDER BY day DESC LIMIT 10").all()
        : { results: [] };
      return Response.json({ scrape: res, d1_rows: rows.results });
    }
    return new Response("puzzle-scraper test worker ok");
  },
  async scheduled(event, env, ctx) {
    ctx.waitUntil(scrape(env, Object.keys(SOURCES), true));
  },
};
