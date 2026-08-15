// Regenerates assets/dsh-desktop.ico (multi-size, PNG-framed) and
// assets/icon-64.rgba (window icon) from the DeepSeek whale path.
// The path is the same one dsh's favicon.svg uses; brand blue #3964fe.
// Usage: node tools/make-icon.mjs
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";

const BRAND = "#3964fe";
const WHALE_D = "M48.8354 10.0479C48.3232 9.79199 48.1025 10.2798 47.8032 10.5278C47.7007 10.6079 47.6143 10.7119 47.5273 10.8076C46.7793 11.624 45.9048 12.1597 44.7622 12.0957C43.0923 12 41.666 12.5356 40.4058 13.8398C40.1377 12.2319 39.2476 11.272 37.8926 10.6558C37.1836 10.3359 36.4668 10.0156 35.9702 9.31982C35.6235 8.82373 35.5293 8.27197 35.356 7.72754C35.2456 7.3999 35.1353 7.06396 34.7651 7.00781C34.3633 6.94385 34.2056 7.2876 34.0479 7.57568C33.418 8.75195 33.1733 10.0479 33.1973 11.3599C33.2524 14.312 34.4736 16.6641 36.8999 18.3359C37.1758 18.5278 37.2466 18.7197 37.1597 19C36.9946 19.5757 36.7974 20.1357 36.624 20.7119C36.5137 21.0801 36.3486 21.1597 35.9624 21C34.6309 20.4321 33.481 19.5918 32.4644 18.5757C30.7393 16.8721 29.1792 14.9917 27.2334 13.52C26.7764 13.1758 26.3193 12.856 25.8467 12.5518C23.8618 10.584 26.1069 8.96777 26.627 8.77588C27.1704 8.57568 26.8159 7.8877 25.0591 7.896C23.3022 7.90381 21.6953 8.50391 19.647 9.30371C19.3477 9.42383 19.0322 9.51172 18.7095 9.58398C16.8501 9.22363 14.9199 9.14355 12.9033 9.37598C9.10596 9.80762 6.07275 11.6396 3.84326 14.7681C1.16455 18.5278 .53418 22.7998 1.30664 27.2559C2.11768 31.9521 4.46582 35.8398 8.07373 38.8799C11.8159 42.0322 16.1255 43.5762 21.041 43.2803C24.0269 43.104 27.3516 42.6963 31.1016 39.4561C32.0469 39.936 33.0396 40.1279 34.686 40.272C35.9546 40.3921 37.1758 40.208 38.1211 40.0078C39.6021 39.688 39.4995 38.2881 38.9639 38.0322C34.623 35.9678 35.5762 36.8081 34.71 36.1279C36.9155 33.4639 40.2402 30.6958 41.54 21.728C41.6426 21.0161 41.5557 20.5679 41.54 19.9917C41.5322 19.6396 41.6108 19.5039 42.0049 19.4639C43.0923 19.3359 44.1479 19.0317 45.1167 18.4878C47.9292 16.9199 49.064 14.3438 49.3315 11.2559C49.3711 10.7837 49.3237 10.2959 48.8354 10.0479ZM24.3262 37.8398C20.1196 34.4639 18.0791 33.3521 17.2358 33.3999C16.4482 33.4482 16.5898 34.3633 16.7632 34.9678C16.9443 35.5601 17.1816 35.9683 17.5117 36.4878C17.7402 36.832 17.8979 37.3428 17.2832 37.728C15.9282 38.5845 13.5728 37.4399 13.4624 37.3838C10.7207 35.7358 8.42822 33.5601 6.81348 30.584C5.25342 27.7197 4.34766 24.6479 4.19775 21.3677C4.1582 20.5757 4.38672 20.2959 5.15869 20.1514C6.17529 19.96 7.22168 19.9199 8.23926 20.0718C12.5327 20.7119 16.1885 22.6719 19.2529 25.7754C21.002 27.5439 22.3252 29.6558 23.6885 31.7202C25.1377 33.9121 26.6978 36 28.6831 37.7119C29.3843 38.312 29.9434 38.7681 30.479 39.104C28.8643 39.2881 26.1699 39.3281 24.3262 37.8398ZM26.3433 24.6001C26.3433 24.248 26.6191 23.9678 26.9658 23.9678C27.0444 23.9678 27.1152 23.9839 27.1782 24.0078C27.2656 24.04 27.3438 24.0879 27.4067 24.1602C27.5171 24.272 27.5806 24.4321 27.5806 24.6001C27.5806 24.9521 27.3047 25.2319 26.958 25.2319C26.6113 25.2319 26.3433 24.9521 26.3433 24.6001ZM32.6064 27.8799C32.2046 28.0479 31.8027 28.1919 31.416 28.208C30.8179 28.2397 30.1646 27.9922 29.8101 27.6885C29.2583 27.2158 28.8643 26.9521 28.6987 26.1279C28.6279 25.7759 28.6675 25.2319 28.7305 24.9199C28.8721 24.248 28.7144 23.8159 28.2495 23.4238C27.8716 23.104 27.3916 23.0161 26.8638 23.0161C26.6665 23.0161 26.4849 22.9277 26.3511 22.856C26.1304 22.7441 25.9492 22.4639 26.1226 22.1201C26.1777 22.0078 26.4453 21.7358 26.5083 21.688C27.2256 21.272 28.0522 21.4077 28.8169 21.7197C29.5254 22.0161 30.0615 22.5601 30.834 23.3281C31.6216 24.2559 31.7632 24.5117 32.2124 25.208C32.5669 25.752 32.8906 26.312 33.1108 26.9521C33.2446 27.3521 33.0713 27.6802 32.6064 27.8799Z";

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const x of buf) c = crcTable[(c ^ x) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const t = Buffer.from(type, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([t, data])));
  return Buffer.concat([len, t, data, crc]);
}
function png(w, h, rgba) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; ihdr[9] = 6;
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 4 + 1)] = 0;
    rgba.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4);
  }
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), chunk("IDAT", zlib.deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}
function parsePath(d) {
  // Numbers may omit the leading zero (".53418"), so accept both forms.
  const toks = d.match(/[MCZ]|-?(?:\d+\.?\d*|\.\d+)(?:e-?\d+)?/g) || [];
  const subs = [];
  let cur = null, start = null, i = 0;
  while (i < toks.length) {
    const t = toks[i];
    if (t === "M") {
      cur = { x: +toks[i + 1], y: +toks[i + 2] };
      start = { ...cur };
      subs.push([cur]);
      i += 3;
    } else if (t === "C") {
      const p1 = { x: +toks[i + 1], y: +toks[i + 2] };
      const p2 = { x: +toks[i + 3], y: +toks[i + 4] };
      const p3 = { x: +toks[i + 5], y: +toks[i + 6] };
      const SEG = 10;
      for (let s = 1; s <= SEG; s++) {
        const u = s / SEG, v = 1 - u;
        subs[subs.length - 1].push({
          x: v * v * v * cur.x + 3 * v * v * u * p1.x + 3 * v * u * u * p2.x + u * u * u * p3.x,
          y: v * v * v * cur.y + 3 * v * v * u * p1.y + 3 * v * u * u * p2.y + u * u * u * p3.y,
        });
      }
      cur = p3;
      i += 7;
    } else if (t === "Z") {
      if (start) {
        subs[subs.length - 1].push({ ...start });
        cur = { ...start };
      }
      i += 1;
    } else i += 1;
  }
  return subs;
}
function render(size, hex) {
  const SS = 4, N = size * SS;
  // The whale path fills its 50x50 viewBox edge to edge; rendering it full
  // bleed clips the anti-aliased boundary pixels (the tail touches x=50).
  // Inset to 86% and center so the whole silhouette survives.
  const PAD = 0.07, scale = N * (1 - 2 * PAD) / 50, off = N * PAD;
  const subs = parsePath(WHALE_D).map((pts) => pts.map((p) => ({ x: p.x * scale + off, y: p.y * scale + off })));
  const cov = new Float32Array(size * size);
  const r = parseInt(hex.slice(1, 3), 16), g = parseInt(hex.slice(3, 5), 16), b = parseInt(hex.slice(5, 7), 16);
  const edges = [];
  for (const pts of subs) {
    for (let i = 0; i < pts.length - 1; i++) {
      const a = pts[i], c = pts[i + 1];
      if (a.y === c.y) continue;
      // Nonzero winding: the edge's original direction decides the sign.
      const dir = c.y > a.y ? 1 : -1;
      edges.push({ top: a.y < c.y ? a : c, bot: a.y < c.y ? c : a, dir });
    }
  }
  for (let sy = 0; sy < N; sy++) {
    const yc = sy + 0.5;
    const xs = [];
    for (const e of edges) {
      if (yc >= e.top.y && yc < e.bot.y) {
        const t = (yc - e.top.y) / (e.bot.y - e.top.y);
        xs.push({ x: e.top.x + t * (e.bot.x - e.top.x), dir: e.dir });
      }
    }
    if (!xs.length) continue;
    xs.sort((p, q) => p.x - q.x);
    let wind = 0, spanStart = 0;
    for (const e of xs) {
      if (wind === 0) spanStart = e.x;
      wind += e.dir;
      if (wind === 0) {
        const from = Math.max(0, spanStart), to = Math.min(N, e.x);
        for (let x = Math.ceil(from); x < to; x++) cov[(sy / SS | 0) * size + (x / SS | 0)] += 1 / (SS * SS);
      }
    }
  }
  const rgba = Buffer.alloc(size * size * 4);
  for (let i = 0; i < size * size; i++) {
    rgba[i * 4] = r; rgba[i * 4 + 1] = g; rgba[i * 4 + 2] = b;
    rgba[i * 4 + 3] = Math.round(Math.min(1, cov[i]) * 255);
  }
  return rgba;
}
// --- .ico with uncompressed 32-bit BMP (DIB) frames ---
// PNG-compressed frames are only reliably supported for the 256px entry;
// small PNG frames render blank ("transparent") in some shell components.
function dibFrame(size, rgba) {
  // BITMAPINFOHEADER (height doubled: XOR + AND masks), 32bpp, bottom-up rows.
  const header = Buffer.alloc(40);
  header.writeUInt32LE(40, 0);
  header.writeInt32LE(size, 4);
  header.writeInt32LE(size * 2, 8); // XOR mask + AND mask
  header.writeUInt16LE(1, 12); // planes
  header.writeUInt16LE(32, 14); // bpp
  const xor = Buffer.alloc(size * size * 4);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const src = (y * size + x) * 4;
      const dst = ((size - 1 - y) * size + x) * 4; // DIB rows are bottom-up
      xor[dst] = rgba[src + 2]; // B
      xor[dst + 1] = rgba[src + 1]; // G
      xor[dst + 2] = rgba[src]; // R
      xor[dst + 3] = rgba[src + 3]; // A
    }
  }
  // AND mask: 1bpp rows padded to 32 bits; the alpha channel decides, so an
  // all-zero (opaque) mask is correct for 32bpp icons.
  const rowBytes = Math.ceil(size / 32) * 4;
  return Buffer.concat([header, xor, Buffer.alloc(rowBytes * size)]);
}
function ico(frames) {
  const out = Buffer.alloc(6 + 16 * frames.length);
  out.writeUInt16LE(0, 0); out.writeUInt16LE(1, 2); out.writeUInt16LE(frames.length, 4);
  let off = 6 + 16 * frames.length;
  const blobs = [];
  frames.forEach(([size, buf], i) => {
    const rec = out.subarray(6 + i * 16, 6 + (i + 1) * 16);
    rec[0] = size >= 256 ? 0 : size;
    rec[1] = size >= 256 ? 0 : size;
    rec.writeUInt16LE(1, 4); rec.writeUInt16LE(32, 6);
    rec.writeUInt32LE(buf.length, 8);
    rec.writeUInt32LE(off, 12);
    blobs.push(buf);
    off += buf.length;
  });
  return Buffer.concat([out, ...blobs]);
}

// --- .icns (Apple icon) with PNG-compressed entries ---
// Modern macOS accepts PNG payloads for every icon type; the size-tagged
// types cover the dock/Finder set including @2x variants.
function icns(entries) {
  // Entry: [type, pngBuffer]. Header: magic "icns" + total file length.
  const body = Buffer.concat(entries.map(([type, buf]) => {
    const head = Buffer.alloc(8);
    head.write(type, 0, "ascii");
    head.writeUInt32BE(buf.length + 8, 4);
    return Buffer.concat([head, buf]);
  }));
  const head = Buffer.alloc(8);
  head.write("icns", 0, "ascii");
  head.writeUInt32BE(body.length + 8, 4);
  return Buffer.concat([head, body]);
}

const root = path.resolve(import.meta.dirname, "..");
const sizes = [16, 24, 32, 48, 64, 128, 256];
fs.writeFileSync(path.join(root, "assets", "dsh-desktop.ico"), ico(sizes.map((s) => [s, dibFrame(s, render(s, BRAND))])));
fs.writeFileSync(path.join(root, "assets", "icon-64.rgba"), render(64, BRAND));
// icns type codes (one entry each; macOS derives @1x/@2x from the type):
// ic11=32, ic12=64, ic07=128, ic08=256, ic09=512, ic10=1024.
const icnsSizes = [[32, "ic11"], [64, "ic12"], [128, "ic07"], [256, "ic08"], [512, "ic09"], [1024, "ic10"]];
fs.writeFileSync(
  path.join(root, "assets", "dsh-desktop.icns"),
  icns(icnsSizes.map(([s, type]) => [type, png(s, s, render(s, BRAND))])),
);
console.log("assets/dsh-desktop.ico, assets/icon-64.rgba and assets/dsh-desktop.icns regenerated");
