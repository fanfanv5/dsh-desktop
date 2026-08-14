// Minimal HTTP(S) downloader used inside the sandbox (curl's schannel is broken).
// Usage: node tools/fetch.mjs <url> <out-file>
import { writeFileSync } from 'node:fs';
const [url, out] = process.argv.slice(2);
const res = await fetch(url, {
  redirect: 'follow',
  headers: { 'user-agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36' },
});
if (!res.ok) { console.error('HTTP', res.status, 'for', url); process.exit(1); }
const buf = Buffer.from(await res.arrayBuffer());
writeFileSync(out, buf);
console.log('HTTP', res.status, '->', out, buf.length, 'bytes', 'head', buf.slice(0, 2).toString('hex'), 'ct', res.headers.get('content-type'));
