// Worker-thread bootstrap for boxentriq's ma_worker.js bundle.
// Shims the browser-worker globals the webpack bundle expects, then loads it.
// The bundle reads its wasm + data profiles via fetch() of root-relative URLs,
// which we resolve against the local harness HTTP server (workerData.origin).
'use strict';
const { parentPort, workerData } = require('worker_threads');
const path = require('path');

const ORIGIN = workerData.origin;
const WORKER_URL = ORIGIN + '/ma_worker.js';

// --- fetch shim: resolve root-relative URLs against the harness server ---
const realFetch = globalThis.fetch.bind(globalThis);
globalThis.fetch = (input, init) => {
  let url = typeof input === 'string' ? input : input.url;
  if (url.startsWith('/')) url = ORIGIN + url;
  return realFetch(typeof input === 'string' ? url : new Request(url, input), init);
};

// --- publicPath shims: webpack checks r.g.importScripts && r.g.location ---
globalThis.importScripts = () => {
  throw new Error('importScripts not supported in this harness');
};
globalThis.location = WORKER_URL;

// --- self shim: onmessage/postMessage wired to parentPort, plus location ---
const selfShim = {
  location: WORKER_URL,
  postMessage: (msg) => parentPort.postMessage(msg),
  _handler: null,
};
Object.defineProperty(selfShim, 'onmessage', {
  configurable: true,
  enumerable: true,
  get() { return this._handler; },
  set(h) { this._handler = h; },
});
parentPort.on('message', (msg) => {
  const h = selfShim._handler;
  if (h) {
    try { h({ data: msg }); } catch (e) { selfShim.postMessage({ kind: 'error', payload: { message: 'onmessage handler threw: ' + (e && e.message) } }); }
  }
});
globalThis.self = selfShim;

// Optional debug: log every fetch URL the worker requests (off by default).
if (workerData.logFetches) {
  const f = globalThis.fetch;
  globalThis.fetch = (input, init) => {
    const url = typeof input === 'string' ? input : input.url;
    parentPort.postMessage({ kind: '__debug_fetch', url });
    return f(input, init);
  };
}

// Load the actual boxentriq worker bundle (black box).
require(path.resolve(__dirname, '..', 'ma_worker.js'));
