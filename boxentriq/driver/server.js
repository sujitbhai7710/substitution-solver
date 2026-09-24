// Static file server for the boxentriq black-box benchmark harness.
// Serves the boxentriq/ directory over HTTP so the worker bundle's fetch()
// calls (data .bin profiles, .wasm module) work under Node.
'use strict';
const http = require('http');
const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '..');
const MIME = {
  '.wasm': 'application/wasm',
  '.js': 'text/javascript',
  '.bin': 'application/octet-stream',
  '.html': 'text/html',
  '.json': 'application/json',
  '.css': 'text/css',
};

function start(port = 0, host = '127.0.0.1') {
  return new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      try {
        const urlPath = decodeURIComponent(req.url.split('?')[0].split('#')[0]);
        const safe = path.normalize(urlPath).replace(/^(\.\.[\/\\])+/, '');
        const file = path.join(ROOT, safe);
        if (!file.startsWith(ROOT)) {
          res.writeHead(403); res.end('forbidden'); return;
        }
        fs.stat(file, (err, st) => {
          if (err || !st.isFile()) {
            res.writeHead(404); res.end('not found'); return;
          }
          res.writeHead(200, {
            'Content-Type': MIME[path.extname(file).toLowerCase()] || 'application/octet-stream',
            'Content-Length': st.size,
          });
          fs.createReadStream(file).pipe(res);
        });
      } catch (e) {
        res.writeHead(500); res.end('error');
      }
    });
    server.listen(port, host, () => {
      resolve({ server, origin: `http://${host}:${server.address().port}` });
    });
    server.on('error', reject);
  });
}

module.exports = { start, ROOT };

if (require.main === module) {
  start(0).then(({ origin }) => console.log('serving at', origin));
}
