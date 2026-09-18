// 生成应用图标源图（1024x1024 PNG）。
// 只用 Node 内置 zlib 手工编码 PNG，不引入任何图形依赖。
//
//   node scripts/make-icon.mjs
//
// 产出 scripts/icon-source.png，再交给 `tauri icon` 生成各平台图标。

import { deflateSync } from 'node:zlib';
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const SIZE = 1024;

// ---------- PNG 编码 ----------

const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i += 1) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const typeBuf = Buffer.from(type, 'ascii');
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])));
  return Buffer.concat([length, typeBuf, data, crc]);
}

function encodePng(width, height, rgba) {
  const raw = Buffer.alloc(height * (width * 4 + 1));
  for (let y = 0; y < height; y += 1) {
    raw[y * (width * 4 + 1)] = 0; // filter: none
    rgba.copy(raw, y * (width * 4 + 1) + 1, y * width * 4, (y + 1) * width * 4);
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type: RGBA
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

// ---------- 绘图基元 ----------

const clamp01 = (v) => (v < 0 ? 0 : v > 1 ? 1 : v);
const smoothstep = (edge0, edge1, x) => {
  const t = clamp01((x - edge0) / (edge1 - edge0));
  return t * t * (3 - 2 * t);
};
const mix = (a, b, t) => a + (b - a) * t;

/** 圆角矩形覆盖率（x/y 相对于中心，正数在内） */
function roundedRectCoverage(x, y, halfW, halfH, radius) {
  const qx = Math.abs(x) - (halfW - radius);
  const qy = Math.abs(y) - (halfH - radius);
  const outside = Math.hypot(Math.max(qx, 0), Math.max(qy, 0));
  const d = Math.min(Math.max(qx, qy), 0) + outside - radius;
  return 1 - smoothstep(-1, 1, d);
}

/** 圆环扇形覆盖率，角度用「数学坐标系」（y 轴向上），单位为度 */
function ringSectorCoverage(x, y, rInner, rOuter, startDeg, endDeg) {
  const dist = Math.hypot(x, y);
  if (dist < rInner - 2 || dist > rOuter + 2) return 0;
  if (dist === 0) return 0;

  let angle = (Math.atan2(-y, x) * 180) / Math.PI;
  if (angle < 0) angle += 360;

  const toPixels = (deg) => (deg * Math.PI * dist) / 180;
  const d = Math.min(
    dist - rInner,
    rOuter - dist,
    toPixels(angle - startDeg),
    toPixels(endDeg - angle),
  );
  return smoothstep(-0.8, 0.8, d);
}

// ---------- 绘制 ----------

const BG_TOP = [30, 64, 175]; // indigo-800
const BG_BOTTOM = [14, 116, 144]; // cyan-700
const GLYPH = [255, 255, 255];

const HALF = SIZE / 2;
const CORNER_RADIUS = SIZE * 0.22;

// 三段等分圆弧组成缺口圆环，缺口即「流转」的语义
const RING_INNER = SIZE * 0.256;
const RING_OUTER = SIZE * 0.33;
const ARC_SPAN = 78;
const SEGMENTS = [
  [0, ARC_SPAN],
  [120, 120 + ARC_SPAN],
  [240, 240 + ARC_SPAN],
];

// 中心圆点
const CENTER_DOT_RADIUS = SIZE * 0.06;

const pixels = Buffer.alloc(SIZE * SIZE * 4);

for (let y = 0; y < SIZE; y += 1) {
  for (let x = 0; x < SIZE; x += 1) {
    // 以中心为原点，y 轴向上
    const cx = x - HALF + 0.5;
    const cy = HALF - y - 0.5;

    const bg = roundedRectCoverage(cx, cy, HALF, HALF, CORNER_RADIUS);
    if (bg <= 0) continue;

    const gradient = smoothstep(0, 1, (-cy + HALF) / SIZE);
    let r = mix(BG_TOP[0], BG_BOTTOM[0], gradient);
    let g = mix(BG_TOP[1], BG_BOTTOM[1], gradient);
    let b = mix(BG_TOP[2], BG_BOTTOM[2], gradient);

    let glyph = 0;
    for (const [start, end] of SEGMENTS) {
      glyph = Math.max(glyph, ringSectorCoverage(cx, cy, RING_INNER, RING_OUTER, start, end));
    }
    glyph = Math.max(
      glyph,
      1 - smoothstep(CENTER_DOT_RADIUS - 1, CENTER_DOT_RADIUS + 1, Math.hypot(cx, cy)),
    );

    r = mix(r, GLYPH[0], glyph);
    g = mix(g, GLYPH[1], glyph);
    b = mix(b, GLYPH[2], glyph);

    const offset = (y * SIZE + x) * 4;
    pixels[offset] = Math.round(r);
    pixels[offset + 1] = Math.round(g);
    pixels[offset + 2] = Math.round(b);
    pixels[offset + 3] = Math.round(bg * 255);
  }
}

const scriptDir = dirname(fileURLToPath(import.meta.url));
const target = join(scriptDir, 'icon-source.png');
writeFileSync(target, encodePng(SIZE, SIZE, pixels));
console.log(`已生成 ${target}`);
