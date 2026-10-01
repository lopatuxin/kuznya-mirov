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
