// Карты кусков грунтовых дорог Транспортной компании («Фаза-43-вид-дороги»): цвет с прозрачностью, мелкий
// рельеф и шероховатость прямого куска каждого вида и узла. Основа — грунт ambientCG Ground103 в настоящем
// размере; поверх — колеи там, где ходят колёса, островки травы, куда колесо не наступает, и неровный край,
// уходящий в землю. Колеи — широкие мягкие ложбины разной силы, каждая виляет сама: ровные тёмные полосы
// читаются сверху как рельсы.
// Картинка куска: по ширине — поперёк дороги, по высоте — вдоль, от начала куска к концу. Всё общее у вариантов
// повторяется с длиной куска, а своё у варианта гаснет к концам: варианты стыкуются в любом порядке без шва.
// Запуск из tools/art: node roads.mjs [папка]  — по умолчанию art/transport/textures.

import { mkdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const HERE = dirname(fileURLToPath(import.meta.url));
const GROUND = resolve(HERE, "../../art/transport/textures/Ground103");
const TILE = 2.5;            // Ground103 покрывает 2,5 × 2,5 метра
const ACROSS = 512;          // точек куска поперёк
const ALONG = 1024;          // точек куска вдоль
const NODE = 512;            // точек узла по стороне
const NODE_SIZE = 9;         // метров — ширина узла, под которую подобран шаг основы

// Виды дорог в метрах: ширина и длина куска; колёса — где ходят, насколько глубока колея (0–1) и её ширина;
// полуширина укатанного грунта и ширина края; трава — сколько её на островках между колёсами.
const KINDS = {
  // Главная улица и тракт: широкая, вытоптанная, колеи двух сторон расплылись в полосы.
  main: { width: 12, length: 16, wheels: [-3.8, -2.2, 2.2, 3.8], rut: 0.45, rutWidth: 0.9, worn: 4.5, edge: 1.5, grass: 0.15 },
  // Улица: колеи двух сторон, между сторонами — островки травы.
  street: { width: 7, length: 14, wheels: [-2.5, -1.0, 1.0, 2.5], rut: 0.55, rutWidth: 0.6, worn: 2.6, edge: 0.9, grass: 0.55 },
  // Проезд: одна пара колей, трава между ними.
  drive: { width: 4.5, length: 9, wheels: [-0.75, 0.75], rut: 0.85, rutWidth: 0.45, worn: 1.3, edge: 0.7, grass: 0.9 },
};
const VARIANTS = ["a", "b"];

// Шум по решётке: значения в узлах из хэша, между ними — сглаженно; по `y` повторяется с периодом `py` узлов.
function hash(x, y, seed) {
  let h = (x * 374761393 + y * 668265263 + seed * 2147483647) | 0;
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}
const smooth = (t) => t * t * (3 - 2 * t);
function lattice(x, y, py, seed) {
  const ix = Math.floor(x), iy = Math.floor(y);
  const fx = smooth(x - ix), fy = smooth(y - iy);
  const at = (i, j) => hash(i, ((j % py) + py) % py, seed);
  const top = at(ix, iy) * (1 - fx) + at(ix + 1, iy) * fx;
  const bottom = at(ix, iy + 1) * (1 - fx) + at(ix + 1, iy + 1) * fx;
  return top * (1 - fy) + bottom * fy;
}
// Несколько октав от −1 до 1; `x`, `y` — в метрах, `cell` — шаг крупной октавы, по `y` период `period` метров.
function fbm(x, y, cell, period, seed, octaves = 3) {
  let sum = 0, amp = 1, norm = 0, c = cell;
  for (let o = 0; o < octaves; o++) {
    const py = Math.max(1, Math.round(period / c));
    sum += amp * (lattice(x / c, (y / period) * py, py, seed + o * 101) * 2 - 1);
    norm += amp;
    amp *= 0.5;
    c /= 2;
  }
  return sum / norm;
}

async function readMap(name) {
  const { data, info } = await sharp(join(GROUND, name)).raw().toBuffer({ resolveWithObject: true });
  return { data, w: info.width, h: info.height, ch: info.channels };
}
// Точка карты основы по метрам, с повтором и сглаживанием; `tile` — сколько метров покрывает карта.
function sampler(map, tile) {
  return (xm, ym, c) => {
    const fx = ((((xm / tile) % 1) + 1) % 1) * map.w - 0.5, fy = ((((ym / tile) % 1) + 1) % 1) * map.h - 0.5;
    const x0 = Math.floor(fx), y0 = Math.floor(fy), tx = fx - x0, ty = fy - y0;
    const px = (x, y) => map.data[((((y % map.h) + map.h) % map.h) * map.w + (((x % map.w) + map.w) % map.w)) * map.ch + c];
    return (px(x0, y0) * (1 - tx) + px(x0 + 1, y0) * tx) * (1 - ty) + (px(x0, y0 + 1) * (1 - tx) + px(x0 + 1, y0 + 1) * tx) * ty;
  };
}

const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
const step = (a, b, v) => smooth(clamp((v - a) / (b - a)));

/** Карты одной точки грунта: цвет основы, тон, затемнение колеёй, прозрачность, шероховатость, высота. */
function texel(maps, p, sample, xm, ym, look) {
  const { rgba, nrm, orm, height } = maps;
  for (let c = 0; c < 3; c++) rgba[p * 4 + c] = clamp((sample.color(xm, ym, c) / 255) * look.shade, 0, 1) * 255;
  rgba[p * 4 + 3] = clamp(look.alpha) * 255;
  orm[p * 3] = 255;
  orm[p * 3 + 1] = clamp(sample.arm(xm, ym, 1) / 255 - look.smooth, 0.62, 1) * 255;
  orm[p * 3 + 2] = 0;
  for (let c = 0; c < 3; c++) nrm[p * 3 + c] = sample.normal(xm, ym, c);
  height[p] = look.height;
}

function newMaps(w, h) {
  return { rgba: Buffer.alloc(w * h * 4), nrm: Buffer.alloc(w * h * 3), orm: Buffer.alloc(w * h * 3), height: new Float32Array(w * h), w, h };
}

function samplerSet(ground, tile) {
  return { color: sampler(ground.color, tile), normal: sampler(ground.normal, tile), arm: sampler(ground.arm, tile) };
}

/** Тон грунта, общий у кусков и узла: крупные пятна светлее и темнее. */
const tone = (n) => 1.16 + 0.07 * n;

/** Карты куска вида `kind`, вариант `variant`. */
function piece(kind, variant, ground) {
  const k = KINDS[kind];
  const sample = samplerSet(ground, k.length / Math.max(1, Math.round(k.length / TILE)));  // целое число повторов основы на кусок
  const own = 17 + VARIANTS.indexOf(variant) * 1000;
  const maps = newMaps(ACROSS, ALONG);
  const dx = k.width / ACROSS, dy = k.length / ALONG;
  for (let j = 0; j < ALONG; j++) {
    const y = (j + 0.5) * dy;
    const window = Math.sin((Math.PI * y) / k.length) ** 2;   // своё у варианта гаснет к концам
    // Шум варианта к концам куска сменяется общим.
    const varied = (x, cell, seed, oct = 3) => fbm(x, y, cell, k.length, seed, oct) * (1 - window) + fbm(x, y, cell, k.length, own + seed, oct) * window;
    // У каждого колеса свой ход, сила колеи и ширина меняются вдоль.
    const wheels = k.wheels.map((w, n) => ({
      at: w + 0.18 * varied(n * 7, 4.5, 60 + n, 2),
      depth: k.rut * clamp(0.65 + 0.55 * varied(n * 7, 3, 70 + n, 2)),
      half: (k.rutWidth / 2) * (0.8 + 0.35 * varied(n * 7, 2.5, 80 + n, 2)),
    }));
    const edgeShift = 0.35 * varied(5, 3, 7, 3);
    for (let i = 0; i < ACROSS; i++) {
      const x = (i + 0.5) * dx - k.width / 2;
      const p = j * ACROSS + i;
      let rut = 0, near = Infinity;
      for (const w of wheels) {
        const d = Math.abs(x - w.at);
        rut = Math.max(rut, w.depth * Math.exp(-((d / w.half) ** 2)));
        near = Math.min(near, d - w.half);
      }
      // Трава островками там, куда колесо не наступает: дальше полутора колей от колеса и внутри укатанного.
      const away = step(0.1, 0.6, near);
      const patch = clamp((varied(x, 1.4, 11) + 0.15) * 2.4) * clamp(0.6 + 0.8 * varied(x, 0.3, 13, 2));
      const grass = k.grass * away * patch;
      // Край: укатанный грунт до `worn`, дальше клочьями уходит в землю.
      const reach = k.worn + edgeShift + 0.3 * varied(x, 0.7, 5);
      const edge = 1 - step(reach, reach + k.edge * 0.7, Math.abs(x));
      const border = 1 - step(k.width / 2 - 0.2, k.width / 2, Math.abs(x));
      const damp = rut * window * clamp(varied(x, 2, 23) * 2.5 - 0.4);
      texel(maps, p, sample, x + k.width / 2, y, {
        shade: tone(varied(x, 5, 31)) * (1 - 0.24 * rut) * (1 - 0.18 * damp),
        alpha: edge * border * (1 - grass),
        smooth: 0.05 * rut + 0.2 * damp,
        height: -0.05 * rut,
      });
    }
  }
  addSlopes(maps, dx, dy, true);
  return maps;
}

/** Карты узла: круг укатанного грунта без колей того же тона, край клочьями уходит в землю. */
function node(ground) {
  const sample = samplerSet(ground, NODE_SIZE / Math.round(NODE_SIZE / TILE));
  const maps = newMaps(NODE, NODE);
  const d = NODE_SIZE / NODE;
  for (let j = 0; j < NODE; j++) {
    for (let i = 0; i < NODE; i++) {
      const x = (i + 0.5) * d - NODE_SIZE / 2, y = (j + 0.5) * d - NODE_SIZE / 2;
      const r = Math.hypot(x, y) / NODE_SIZE;  // 0,5 — край картинки
      const a = Math.atan2(y, x);
      const reach = 0.37 + 0.03 * Math.sin(3 * a + 1.1) + 0.02 * Math.sin(5 * a + 2.3) + 0.035 * fbm(x, y, 0.7, 1000, 43);
      texel(maps, j * NODE + i, sample, x + NODE_SIZE / 2, y + NODE_SIZE / 2, {
        shade: tone(fbm(x, y, 5, 1000, 47)),
        alpha: (1 - step(reach, reach + 0.1, r)) * (1 - step(0.48, 0.5, r)),
        smooth: 0,
        height: 0,
      });
    }
  }
  addSlopes(maps, d, d, false);
  return maps;
}

/** Наклоны высоты добавляются к карте рельефа: +X — вправо по картинке, +Y — вверх по картинке, как у glTF;
 *  `wrap` — вдоль картинка повторяется. */
function addSlopes(maps, dx, dy, wrap) {
  const { nrm, height, w, h } = maps;
  const row = (j) => (wrap ? (j + h) % h : Math.min(h - 1, Math.max(0, j)));
  for (let j = 0; j < h; j++) {
    for (let i = 0; i < w; i++) {
      const p = j * w + i;
      const hx = (height[j * w + Math.min(w - 1, i + 1)] - height[j * w + Math.max(0, i - 1)]) / (2 * dx);
      const hy = (height[row(j - 1) * w + i] - height[row(j + 1) * w + i]) / (2 * dy);
      let nx = nrm[p * 3] / 127.5 - 1 - hx, ny = nrm[p * 3 + 1] / 127.5 - 1 - hy, nz = nrm[p * 3 + 2] / 127.5 - 1;
      const len = Math.hypot(nx, ny, nz);
      nrm[p * 3] = (nx / len + 1) * 127.5;
      nrm[p * 3 + 1] = (ny / len + 1) * 127.5;
      nrm[p * 3 + 2] = (nz / len + 1) * 127.5;
    }
  }
}

async function write(dir, maps) {
  await mkdir(dir, { recursive: true });
  const raw = (channels) => ({ raw: { width: maps.w, height: maps.h, channels } });
  await sharp(maps.rgba, raw(4)).png({ compressionLevel: 9 }).toFile(join(dir, "color.png"));
  await sharp(maps.nrm, raw(3)).jpeg({ quality: 92 }).toFile(join(dir, "normal.jpg"));
  await sharp(maps.orm, raw(3)).jpeg({ quality: 92 }).toFile(join(dir, "arm.jpg"));
}

const out = resolve(process.argv[2] ?? resolve(HERE, "../../art/transport/textures"));
const ground = { color: await readMap("color.jpg"), normal: await readMap("normal.jpg"), arm: await readMap("arm.jpg") };
for (const kind of Object.keys(KINDS)) {
  for (const variant of VARIANTS) await write(join(out, `road_${kind}_${variant}`), piece(kind, variant, ground));
}
await write(join(out, "road_node"), node(ground));
console.log(`карты дорог — в ${out}`);
