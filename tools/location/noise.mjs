// Плавные неровности земли: градиентный шум Перлина и его сумма по нескольким масштабам.

import { random } from "./random.mjs";

const GRADIENTS = Array.from({ length: 16 }, (_, i) => {
  const angle = (i / 16) * Math.PI * 2;
  return [Math.cos(angle), Math.sin(angle)];
});

function fade(t) {
  return t * t * t * (t * (t * 6 - 15) + 10);
}

/** Шум от −1 до 1 с волной в единицу: `noise(x, y)`. Зерно задаёт перестановку узлов. */
export function perlin(seed) {
  const next = random(seed);
  const order = Array.from({ length: 256 }, (_, i) => i);
  for (let i = 255; i > 0; i--) {
    const j = Math.floor(next() * (i + 1));
    [order[i], order[j]] = [order[j], order[i]];
  }
  const perm = [...order, ...order];
  const corner = (ix, iy, dx, dy) => {
    const g = GRADIENTS[perm[perm[ix & 255] + (iy & 255)] & 15];
    return g[0] * dx + g[1] * dy;
  };
  return (x, y) => {
    const ix = Math.floor(x);
    const iy = Math.floor(y);
    const fx = x - ix;
    const fy = y - iy;
    const u = fade(fx);
    const v = fade(fy);
    const top = corner(ix, iy, fx, fy) + u * (corner(ix + 1, iy, fx - 1, fy) - corner(ix, iy, fx, fy));
    const bottom = corner(ix, iy + 1, fx, fy - 1) + u * (corner(ix + 1, iy + 1, fx - 1, fy - 1) - corner(ix, iy + 1, fx, fy - 1));
    // Градиентный шум в двух измерениях не выходит за ±√½; растяжение даёт почти весь отрезок ±1.
    return Math.SQRT2 * (top + v * (bottom - top));
  };
}

/** Сумма `octaves` масштабов, каждый вдвое мельче и вдвое слабее прошлого; результат от −1 до 1. */
export function fractal(seed, octaves = 4) {
  const layers = Array.from({ length: octaves }, (_, i) => perlin(seed + i * 1013));
  const total = layers.reduce((sum, _, i) => sum + 0.5 ** i, 0);
  return (x, y) => layers.reduce((sum, layer, i) => sum + layer(x * 2 ** i, y * 2 ** i) * 0.5 ** i, 0) / total;
}

/** Хребты: гребни там, где шум переходит через ноль; от 0 в распадках до 1 на гребнях. */
export function ridged(seed, octaves = 4) {
  const layers = Array.from({ length: octaves }, (_, i) => perlin(seed + i * 2029));
  const total = layers.reduce((sum, _, i) => sum + 0.5 ** i, 0);
  return (x, y) => layers.reduce((sum, layer, i) => sum + (1 - Math.abs(layer(x * 2 ** i, y * 2 ** i))) ** 2 * 0.5 ** i, 0) / total;
}

/**
 * Ячейки Вороного с единичным шагом: у каждой клетки единичной решётки своя точка, сдвинутая
 * случайно. Для места `(x, y)` — ячейка, в которой оно лежит (`id` от 0 до 1 — её случайное число),
 * расстояние до её точки `f1` и точное расстояние до ближайшей границы ячеек `border` (Inigo Quilez,
 * «Voronoi — distances»).
 */
export function cells(seed) {
  const hash = (i, j) => {
    let h = (seed ^ Math.imul(i, 0x27d4eb2d) ^ Math.imul(j, 0x165667b1)) >>> 0;
    h = Math.imul(h ^ (h >>> 15), 0x85ebca6b);
    h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
    return (h ^ (h >>> 16)) >>> 0;
  };
  const point = (i, j) => {
    const h = hash(i, j);
    return [i + 0.1 + 0.8 * ((h & 0xffff) / 65536), j + 0.1 + 0.8 * ((h >>> 16) / 65536)];
  };
  return (x, y) => {
    const ix = Math.floor(x);
    const iy = Math.floor(y);
    let best = Infinity;
    let own = [0, 0];
    let bi = 0;
    let bj = 0;
    for (let dj = -1; dj <= 1; dj++) {
      for (let di = -1; di <= 1; di++) {
        const p = point(ix + di, iy + dj);
        const d = (p[0] - x) ** 2 + (p[1] - y) ** 2;
        if (d < best) {
          best = d;
          own = p;
          bi = ix + di;
          bj = iy + dj;
        }
      }
    }
    let border = Infinity;
    for (let dj = -2; dj <= 2; dj++) {
      for (let di = -2; di <= 2; di++) {
        if (di === 0 && dj === 0) continue;
        const p = point(bi + di, bj + dj);
        const nx = p[0] - own[0];
        const ny = p[1] - own[1];
        const length = Math.hypot(nx, ny);
        const mx = (own[0] + p[0]) / 2 - x;
        const my = (own[1] + p[1]) / 2 - y;
        border = Math.min(border, (mx * nx + my * ny) / length);
      }
    }
    return { id: hash(bi, bj) / 4294967296, f1: Math.sqrt(best), border };
  };
}
