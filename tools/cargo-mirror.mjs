// Local plain-HTTP mirror for a crates.io mirror (rsproxy.cn), used only inside
// this sandboxed environment where cargo's schannel TLS backend is blocked and
// direct international HTTPS is unreachable. cargo talks to this server over
// HTTP; this server fetches the China-accessible rsproxy.cn upstream over HTTPS
// via Node's (working) TLS.
//
// Endpoints:
//   GET /index/config.json          -> rewrite dl base, fetch https://rsproxy.cn/index/config.json
//   GET /index/<prefix>/<name>      -> https://rsproxy.cn/index/<prefix>/<name>
//   GET /dl/<crate>/<version>/download -> https://rsproxy.cn/api/v1/crates/<crate>/<version>/download
//
// Usage: node tools/cargo-mirror.mjs [port]

import http from 'node:http';
import https from 'node:https';

const PORT = Number(process.argv[2] || 8899);
const INDEX_BASE = 'https://rsproxy.cn/index';
const DL_BASE = 'https://rsproxy.cn/api/v1/crates';
const LOCAL_DL = 'http://127.0.0.1:' + PORT + '/dl';

// Fetch a small resource fully (used for sparse-index JSON).
function fetchBuffer(url) {
  return new Promise((resolve, reject) => {
    const lib = url.startsWith('https:') ? https : http;
    const req = lib.get(url, { headers: { 'user-agent': 'cargo-mirror/1.0' } }, (res) => {
      if (res.statusCode && res.statusCode >= 400) {
        res.resume();
        reject(new Error('upstream ' + res.statusCode + ' ' + url));
        return;
      }
      const chunks = [];
      res.on('data', (c) => chunks.push(c));
      res.on('end', () => resolve(Buffer.concat(chunks)));
    });
    req.on('error', reject);
    req.setTimeout(120000, () => req.destroy(new Error('timeout')));
  });
}

// Stream a large resource straight through (used for .crate downloads), following redirects.
function pipe(upstream, clientRes, redirects = 0) {
  return new Promise((resolve, reject) => {
    const lib = upstream.startsWith('https:') ? https : http;
    const req = lib.get(upstream, { headers: { 'user-agent': 'cargo-mirror/1.0' } }, (up) => {
      if (up.statusCode && up.statusCode >= 300 && up.statusCode < 400 && up.headers.location) {
        up.resume();
        const next = new URL(up.headers.location, upstream).toString();
        if (redirects < 5) {
          pipe(next, clientRes, redirects + 1).then(resolve, reject);
        } else {
          reject(new Error('too many redirects ' + upstream));
        }
        return;
      }
      if (up.statusCode && up.statusCode >= 400) {
        up.resume();
        reject(new Error('upstream ' + up.statusCode + ' ' + upstream));
        return;
      }
      clientRes.writeHead(up.statusCode || 200, {
        'content-type': up.headers['content-type'] || 'application/octet-stream',
        'content-length': up.headers['content-length'],
        'cache-control': 'no-store',
      });
      up.pipe(clientRes);
      up.on('end', resolve);
      up.on('error', reject);
    });
    req.on('error', reject);
    req.setTimeout(600000, () => req.destroy(new Error('timeout')));
  });
}

const server = http.createServer(async (req, res) => {
  const path = (req.url || '').split('?')[0];
  try {
    if (path === '/health') {
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end('ok');
      return;
    }
    if (path === '/index/config.json') {
      const body = await fetchBuffer(INDEX_BASE + '/config.json');
      const cfg = JSON.parse(body.toString('utf8'));
      cfg.dl = LOCAL_DL;
      const out = Buffer.from(JSON.stringify(cfg));
      res.writeHead(200, { 'content-type': 'application/json', 'content-length': out.length });
      res.end(out);
      return;
    }
    if (path.startsWith('/index/')) {
      const body = await fetchBuffer(INDEX_BASE + path.slice('/index'.length));
      res.writeHead(200, { 'content-type': 'application/json', 'content-length': body.length });
      res.end(body);
      return;
    }
    if (path.startsWith('/dl/')) {
      // cargo requests /dl/{crate}/{version}/download for a replaced registry.
      const m = path.match(/^\/dl\/([^/]+)\/([^/]+)\/download$/);
      if (!m) {
        throw new Error('bad dl path ' + path);
      }
      const upstream = DL_BASE + '/' + m[1] + '/' + m[2] + '/download';
      await pipe(upstream, res);
      return;
    }
    res.writeHead(404);
    res.end('not found');
  } catch (e) {
    if (!res.headersSent) {
      res.writeHead(502, { 'content-type': 'text/plain' });
      res.end('mirror error: ' + e.message);
    } else {
      res.destroy();
    }
  }
});

server.listen(PORT, '127.0.0.1', () => {
  console.log('[cargo-mirror] listening on http://127.0.0.1:' + PORT);
});
server.on('error', (e) => {
  console.error('[cargo-mirror] bind error:', e.message);
  process.exit(1);
});
