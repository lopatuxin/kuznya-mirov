// Рисует свой материал рельефа программой, как рисуют в Substance Designer: сначала высота
// поверхности из крупных форм (пласты, глыбы, трещины, обломки), затем из неё все карты «Свет и
// материалы» — цвет, нормали по соглашению OpenGL, шероховатость, высота и затенение щелей.
// Плитка бесшовная: весь шум и ячейки повторяются с периодом плитки.
// Запуск: `node tools/art/material.mjs <rock|scree> <папка материала> [--size 1024] [--seed N]`.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import sharp from "sharp";

const USAGE = "node tools/art/material.mjs <rock|scree> <папка> [--size 1024] [--seed N]";

/** Целое случайное по трём целым: одни и те же числа — одно и то же значение. */
function hash(a, b, seed) {
  let h = (seed ^ Math.imul(a, 0x27d4eb2d) ^ Math.imul(b, 0x165667b1)) >>> 0;
  h = Math.imul(h ^ (h >>> 15), 0x85ebca6b);
  h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
  return (h ^ (h >>> 16)) >>> 0;
}

function unit(a, b, seed) {
  return hash(a, b, seed) / 4294967296;
}

function wrap(i, period) {
  return ((i % period) + period) % period;
}

function smoothstep(edge0, edge1, x) {
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

/** Градиентный шум от −1 до 1 с периодом `period` клеток решётки по x и `periodY` по y. */
export function periodicNoise(seed, period, periodY = period) {
  const gradient = (i, j) => {
    const angle = unit(wrap(i, period), wrap(j, periodY), seed) * Math.PI * 2;
    return [Math.cos(angle), Math.sin(angle)];
  };
  const fade = (t) => t * t * t * (t * (t * 6 - 15) + 10);
  return (x, y) => {
    const i = Math.floor(x);
    const j = Math.floor(y);
    const fx = x - i;
    const fy = y - j;
    const dot = (di, dj) => {
      const [gx, gy] = gradient(i + di, j + dj);
      return gx * (fx - di) + gy * (fy - dj);
    };
    const u = fade(fx);
    const v = fade(fy);
    const top = dot(0, 0) + u * (dot(1, 0) - dot(0, 0));
    const bottom = dot(0, 1) + u * (dot(1, 1) - dot(0, 1));
    return Math.SQRT2 * (top + v * (bottom - top));
  };
}

/** Сумма `octaves` масштабов периодического шума: `(u, v)` — место в плитке от 0 до 1. */
export function periodicFbm(seed, period, octaves) {
  const layers = Array.from({ length: octaves }, (_, k) => periodicNoise(seed + k * 101, period * 2 ** k));
  const total = layers.reduce((sum, _, k) => sum + 0.5 ** k, 0);
  return (u, v) => layers.reduce((sum, layer, k) => sum + layer(u * period * 2 ** k, v * period * 2 ** k) * 0.5 ** k, 0) / total;
}

/**
 * Ячейки Вороного с периодом `nx × ny` ячеек на плитку: для места `(u, v)` — номер ячейки `id`
 * (0…1), расстояние до её точки `f1` и до ближайшей границы `border`, в долях ширины ячейки.
 */
export function periodicCells(seed, nx, ny) {
  const point = (i, j) => {
    const a = wrap(i, nx);
    const b = wrap(j, ny);
    return [i + 0.15 + 0.7 * unit(a, b, seed), j + 0.15 + 0.7 * unit(a, b, seed + 1)];
  };
  return (u, v) => {
    const x = u * nx;
    const y = v * ny;
    const ix = Math.floor(x);
    const iy = Math.floor(y);
    let best = Infinity;
    let own = null;
    let oi = 0;
    let oj = 0;
    for (let dj = -1; dj <= 1; dj++) {
      for (let di = -1; di <= 1; di++) {
        const p = point(ix + di, iy + dj);
        const d = (p[0] - x) ** 2 + (p[1] - y) ** 2;
        if (d < best) [best, own, oi, oj] = [d, p, ix + di, iy + dj];
      }
    }
    let border = Infinity;
    for (let dj = -2; dj <= 2; dj++) {
      for (let di = -2; di <= 2; di++) {
        if (di === 0 && dj === 0) continue;
        const p = point(oi + di, oj + dj);
        const [nx0, ny0] = [p[0] - own[0], p[1] - own[1]];
        const length = Math.hypot(nx0, ny0);
        const [mx, my] = [(own[0] + p[0]) / 2 - x, (own[1] + p[1]) / 2 - y];
        border = Math.min(border, (mx * nx0 + my * ny0) / length);
      }
    }
    return { id: unit(wrap(oi, nx), wrap(oj, ny), seed + 2), f1: Math.sqrt(best), border, dx: x - own[0], dy: y - own[1] };
  };
}

/** Размытие квадратом со стороной `2r + 1` по кругу плитки — для затенения щелей. */
function blur(values, size, r) {
  const pass = (src, horizontal) => {
    const out = new Float32Array(size * size);
    for (let a = 0; a < size; a++) {
      let sum = 0;
      for (let k = -r; k <= r; k++) sum += horizontal ? src[a * size + wrap(k, size)] : src[wrap(k, size) * size + a];
      for (let b = 0; b < size; b++) {
        const at = horizontal ? a * size + b : b * size + a;
        out[at] = sum / (2 * r + 1);
        const add = wrap(b + r + 1, size);
        const drop = wrap(b - r, size);
        sum += horizontal ? src[a * size + add] - src[a * size + drop] : src[add * size + a] - src[drop * size + a];
      }
    }
    return out;
  };
  return pass(pass(values, true), false);
}

const mix = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);

/**
 * Скала: пласты разной толщины идут поперёк (верх картинки — вверх по стене), волнистые, а швы
 * между ними местами пропадают; косые трещины колют пласты на куски, каждый кусок выдвинут или
 * утоплен и наклонён своей плоскостью, по нему — рубленые грани сколов, по всему — зерно. Цвет:
 * тёплый и холодный серый по кускам, охристые и тёмные пласты, потёки сверху вниз, лишайник.
 */
function rock(size, seed) {
  const warpU = periodicFbm(seed, 2, 4);
  const warpV = periodicFbm(seed + 1, 2, 4);
  const fade = periodicFbm(seed + 2, 3, 3);
  const pieces = periodicCells(seed + 3, 5, 2);
  const facets = periodicCells(seed + 4, 14, 10);
  const chips = periodicFbm(seed + 10, 10, 5);
  const grain = periodicFbm(seed + 20, 64, 2);
  const streaks = periodicNoise(seed + 30, 20, 2);
  const lichen = periodicFbm(seed + 40, 8, 4);
  const bands = 6;
  const thickness = Array.from({ length: bands }, (_, k) => 0.4 + unit(k, 0, seed));
  const total = thickness.reduce((a, b) => a + b, 0);
  const bounds = [0];
  for (const t of thickness) bounds.push(bounds[bounds.length - 1] + t / total);
  const height = new Float32Array(size * size);
  const tone = new Float32Array(size * size * 3);
  const face = new Float32Array(size * size);
  for (let row = 0; row < size; row++) {
    for (let col = 0; col < size; col++) {
      const u = (col + 0.5) / size;
      const v = (row + 0.5) / size;
      const wu = u + 0.05 * warpU(u, v);
      const wv = v + 0.05 * warpV(u, v);
      // Пласты горизонтальны: наклон бесшовной плитки — только целый оборот на плитку, а это 45°.
      const tv = wrap(wv, 1);
      let k = 0;
      while (k < bands - 1 && tv >= bounds[k + 1]) k++;
      const r = (tv - bounds[k]) / (bounds[k + 1] - bounds[k]);
      const seamGap = Math.min(tv - bounds[k], bounds[k + 1] - tv);
      const piece = pieces(wu, wv);
      const pid = hash(k, Math.floor(piece.id * 1e6), seed) / 4294967296;
      let h = 0.5 + 0.25 * (pid - 0.5);
      h += 0.35 * ((unit(k, Math.floor(pid * 1e6), seed + 5) - 0.5) * piece.dx + (unit(k, Math.floor(pid * 1e6), seed + 6) - 0.5) * piece.dy * 0.5);
      h += 0.1 * smoothstep(0.6, 1, 1 - r) - 0.14 * smoothstep(0.85, 1, r);
      const seamDepth = smoothstep(-0.25, 0.25, fade(u, v));
      h -= 0.2 * seamDepth * (1 - smoothstep(0, 0.008, seamGap));
      h -= 0.18 * (1 - smoothstep(0, 0.035, piece.border));
      const facet = facets(wu, wv);
      const fid = Math.floor(facet.id * 1e6);
      h += 0.09 * ((unit(fid, 1, seed) - 0.5) * facet.dx + (unit(fid, 2, seed) - 0.5) * facet.dy) + 0.05 * (facet.id - 0.5);
      h += 0.07 * chips(u, v) + 0.03 * grain(u, v);
      const at = row * size + col;
      height[at] = h;
      face[at] = smoothstep(0.01, 0.06, piece.border) * smoothstep(0.002, 0.01, seamGap);
      const warm = [0.6, 0.57, 0.52];
      const cool = [0.5, 0.51, 0.52];
      const ochre = [0.6, 0.5, 0.38];
      let c = mix(cool, warm, pid);
      c = c.map((x) => x * (0.8 + 0.3 * unit(k, 9, seed)));
      c = mix(c, ochre, 0.25 * smoothstep(0.7, 0.95, unit(k, 6, seed)));
      c = c.map((x) => x * (0.94 + 0.12 * unit(fid, 3, seed)));
      c = c.map((x) => x * (1 - 0.18 * smoothstep(0.05, 0.6, streaks(u * 20, v * 2))));
      const moss = smoothstep(0.2, 0.45, lichen(u, v)) * smoothstep(0.5, 0.9, 1 - r);
      c = mix(c, [0.55, 0.56, 0.4], 0.6 * moss);
      c = c.map((x) => x * (1 + 0.1 * grain(u + 0.5, v)));
      tone.set(c, at * 3);
    }
  }
  return { height, tone, face, relief: 5, roughness: [0.8, 0.95] };
}

/**
 * Осыпь: угловатые обломки двух размеров вплотную друг к другу, каждый — невысокий купол с плоской
 * гранью под своим наклоном; между ними — тёмная земля. Цвет по обломку: светлый и тёплый серый.
 */
function scree(size, seed) {
  const big = periodicCells(seed, 11, 11);
  const small = periodicCells(seed + 1, 27, 27);
  const grain = periodicFbm(seed + 2, 48, 2);
  const warpU = periodicFbm(seed + 7, 8, 3);
  const warpV = periodicFbm(seed + 8, 8, 3);
  const height = new Float32Array(size * size);
  const tone = new Float32Array(size * size * 3);
  const face = new Float32Array(size * size);
  const stone = (cell, seedShift) => {
    const tiltX = unit(Math.floor(cell.id * 1e6), 1, seed + seedShift) - 0.5;
    const tiltY = unit(Math.floor(cell.id * 1e6), 2, seed + seedShift) - 0.5;
    const top = 0.55 + 0.35 * cell.id + 0.5 * (tiltX * cell.dx + tiltY * cell.dy);
    return { h: top * smoothstep(0.03, 0.2, cell.border), edge: smoothstep(0.03, 0.15, cell.border) };
  };
  for (let row = 0; row < size; row++) {
    for (let col = 0; col < size; col++) {
      const u = (col + 0.5) / size;
      const v = (row + 0.5) / size;
      // Сдвиг шумом ломает ровные грани ячеек: обломки неровные, а не плитка.
      const wu = u + 0.012 * warpU(u, v);
      const wv = v + 0.012 * warpV(u, v);
      const a = big(wu, wv);
      const b = small(wu, wv);
      const sa = stone(a, 3);
      const sb = stone(b, 4);
      const onBig = sa.h * 1.0 >= sb.h * 0.7;
      const h = Math.max(sa.h, sb.h * 0.7) + 0.03 * grain(u, v);
      const at = row * size + col;
      height[at] = h;
      face[at] = onBig ? sa.edge : sb.edge;
      const id = onBig ? a.id : b.id;
      let c = mix([0.52, 0.51, 0.5], [0.6, 0.55, 0.47], unit(Math.floor(id * 1e6), 5, seed));
      c = c.map((x) => x * (0.7 + 0.4 * unit(Math.floor(id * 1e6), 6, seed)));
      const soil = [0.24, 0.21, 0.18];
      c = mix(soil, c, smoothstep(0.05, 0.3, h));
      c = c.map((x) => x * (1 + 0.08 * grain(u + 0.3, v)));
      tone.set(c, at * 3);
    }
  }
  return { height, tone, face, relief: 3, roughness: [0.8, 0.95] };
}

const RECIPES = { rock, scree };

/** Все карты материала по высоте и цвету рецепта. */
export function materialMaps(kind, size, seed) {
  const { height, tone, face, relief, roughness } = RECIPES[kind](size, seed);
  let low = Infinity;
  let high = -Infinity;
  for (const h of height) [low, high] = [Math.min(low, h), Math.max(high, h)];
  const h01 = height.map((h) => (h - low) / (high - low));
  const near = blur(h01, size, Math.max(1, Math.round(size / 256)));
  const far = blur(h01, size, Math.round(size / 48));
  const color = Buffer.alloc(size * size * 3);
  const normal = Buffer.alloc(size * size * 3);
  const rough = Buffer.alloc(size * size);
  const heightMap = Buffer.alloc(size * size);
  const ao = Buffer.alloc(size * size);
  const at = (row, col) => h01[wrap(row, size) * size + wrap(col, size)];
  for (let row = 0; row < size; row++) {
    for (let col = 0; col < size; col++) {
      const i = row * size + col;
      const cavity = Math.max(0, near[i] - h01[i]) * 6 + Math.max(0, far[i] - h01[i]) * 2.2;
      const open = Math.max(0.25, 1 - cavity);
      ao[i] = Math.round(255 * open);
      heightMap[i] = Math.round(255 * h01[i]);
      // Наклон на точку картинки в долях высоты; `relief` — во сколько раз рельеф глубже по нормалям.
      const dx = (at(row, col + 1) - at(row, col - 1)) * 0.5 * relief * (size / 64);
      const dy = (at(row + 1, col) - at(row - 1, col)) * 0.5 * relief * (size / 64);
      // OpenGL: зелёный — вверх по картинке; строка растёт вниз, поэтому наклон вверх — это −dy.
      const n = [-dx, dy, 1];
      const length = Math.hypot(...n);
      for (let c = 0; c < 3; c++) normal[i * 3 + c] = Math.round(255 * (n[c] / length * 0.5 + 0.5));
      const shade = 0.88 + 0.12 * open;
      // Тон рецепта — уже в sRGB, как цвет картинки.
      for (let c = 0; c < 3; c++) color[i * 3 + c] = Math.round(255 * Math.min(1, tone[i * 3 + c] * shade));
      rough[i] = Math.round(255 * (roughness[0] + (roughness[1] - roughness[0]) * (1 - face[i])));
    }
  }
  return { color, normal, rough, heightMap, ao };
}

async function main() {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: { size: { type: "string", default: "1024" }, seed: { type: "string", default: "20261001" } },
  });
  const [kind, dir] = positionals;
  if (!RECIPES[kind] || !dir) throw new Error(USAGE);
  const size = Number(values.size);
  if (![512, 1024, 2048].includes(size)) throw new Error("--size — 512, 1024 или 2048: карты всех материалов игры одного размера");
  const maps = materialMaps(kind, size, Number(values.seed));
  mkdirSync(dir, { recursive: true });
  const write = (buffer, channels, name) => sharp(buffer, { raw: { width: size, height: size, channels } }).png().toFile(join(dir, name));
  await Promise.all([
    write(maps.color, 3, "color.png"),
    write(maps.normal, 3, "normal.png"),
    write(maps.rough, 1, "roughness.png"),
    write(maps.heightMap, 1, "height.png"),
    write(maps.ao, 1, "ao.png"),
  ]);
  console.log(`${kind} ${size} × ${size} → ${dir}`);
}

// Тест импортирует функции этого файла, и импорт не должен запускать рисование.
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
