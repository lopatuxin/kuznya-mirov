// Сетка высот локации и операции над ней. Сетка — как в «Рельеф» Кузни: точки через равные доли
// клетки, `density` точек на клетку, крайние на краях сцены; между точками земля — два треугольника
// по диагонали из левого верхнего угла в правый нижний, как у движка (`Terrain::height_at`).

import { fractal, ridged } from "./noise.mjs";

export function smoothstep(edge0, edge1, x) {
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

export class Grid {
  constructor(width, height, density = 2) {
    this.width = width;
    this.height = height;
    this.density = density;
    this.cols = density * width + 1;
    this.rows = density * height + 1;
    this.h = new Float64Array(this.cols * this.rows);
  }

  /** Сторона квадрата сетки в клетках. */
  get step() {
    return 1 / this.density;
  }

  /**
   * Карта `name` по точкам сетки — то, что операции гор копят для покрытий: где текла вода, что она
   * унесла и положила, где лежит осыпь. Первое обращение заводит карту из нулей.
   */
  map(name) {
    this.maps ??= new Map();
    if (!this.maps.has(name)) this.maps.set(name, new Float64Array(this.cols * this.rows));
    return this.maps.get(name);
  }

  /** Тангенс крутизны в точке сетки `index` по высотам `h` (без `h` — по нынешним). */
  slopeTan(index, h = this.h) {
    const col = index % this.cols;
    const row = (index - col) / this.cols;
    const left = col > 0 ? index - 1 : index;
    const right = col < this.cols - 1 ? index + 1 : index;
    const up = row > 0 ? index - this.cols : index;
    const down = row < this.rows - 1 ? index + this.cols : index;
    const gx = ((h[right] - h[left]) * this.density) / Math.max(1, (right - left));
    const gy = ((h[down] - h[up]) * this.density) / Math.max(1, (down - up) / this.cols);
    return Math.hypot(gx, gy);
  }

  at(row, col) {
    return this.h[row * this.cols + col];
  }

  /** Вызывает `fn(index, x, y)` для каждой точки сетки в прямоугольнике `box`, обрезанном краем сцены. */
  each(box, fn) {
    const d = this.density;
    const c0 = Math.max(0, Math.ceil(box.minX * d));
    const c1 = Math.min(this.cols - 1, Math.floor(box.maxX * d));
    const r0 = Math.max(0, Math.ceil(box.minY * d));
    const r1 = Math.min(this.rows - 1, Math.floor(box.maxY * d));
    for (let r = r0; r <= r1; r++) {
      for (let c = c0; c <= c1; c++) fn(r * this.cols + c, c / d, r / d);
    }
  }

  get whole() {
    return { minX: 0, minY: 0, maxX: this.width, maxY: this.height };
  }

  /** Высота земли в месте сцены по треугольникам движка; за краем — высота края. */
  heightAt(x, y) {
    const gx = Math.min(this.cols - 1, Math.max(0, x * this.density));
    const gy = Math.min(this.rows - 1, Math.max(0, y * this.density));
    const col = Math.min(this.cols - 2, Math.floor(gx));
    const row = Math.min(this.rows - 2, Math.floor(gy));
    const u = gx - col;
    const v = gy - row;
    const base = this.at(row, col);
    if (u >= v) return base + u * (this.at(row, col + 1) - base) + v * (this.at(row + 1, col + 1) - this.at(row, col + 1));
    return base + v * (this.at(row + 1, col) - base) + u * (this.at(row + 1, col + 1) - this.at(row + 1, col));
  }
}

// Сумма масштабов шума почти всегда лежит в ±0,5: растянутая вдвое и обрезанная, она доходит до ±1,
// и `warp` и `irregular` задают настоящий размах отрогов и края холма.
export function spread(value) {
  return Math.max(-1, Math.min(1, value * 2));
}

function grown(box, by) {
  return { minX: box.minX - by, minY: box.minY - by, maxX: box.maxX + by, maxY: box.maxY + by };
}

/** Неровности: `amplitude` клеток вверх и вниз волной `wavelength`; в `area` гаснут к её краю на `fade`. */
export function applyNoise(grid, { amplitude, wavelength, seed, area, fade }) {
  const noise = fractal(seed);
  grid.each(area ? area.box : grid.whole, (index, x, y) => {
    const weight = area ? smoothstep(0, fade, -area.signedDistance([x, y])) : 1;
    if (weight > 0) grid.h[index] += amplitude * noise(x / wavelength, y / wavelength) * weight;
  });
}

/**
 * На сколько клеток точка `(x, y)` зашла за подножие гор `range` в их сторону, с отрогами и заливами
 * `warp`: больше нуля — в горах, меньше — перед ними.
 */
export function rangeReach(range) {
  const place = rangePlace(range);
  return (x, y) => place(x, y).reach;
}

/** То же, что `rangeReach`, и `along` — сколько клеток от начала линии подножия до ближайшей к точке её точки. */
function rangePlace({ foot, side, wavelength, warp, seed }) {
  const wobble = fractal(seed + 1);
  return (x, y) => {
    const near = foot.nearest([x, y]);
    const inside = near.left === (side === "left") ? near.distance : -near.distance;
    return { reach: inside + warp * spread(wobble(x / wavelength, y / wavelength)), along: near.s };
  };
}

/** Насколько массивы гор выше и ниже средней высоты при `roughness` 1. */
const MASSIF_SPREAD = 0.35;

/**
 * Горы: от линии подножия `foot` в сторону `side` земля поднимается на `height` на глубине `depth`.
 * Подъём круче всего у подножия — втрое круче среднего `height / depth` — и выполаживается к
 * гребню; с `profile` «middle» он пологий у подножия и у гребня и круче всего посередине.
 * `roughness` — доля высоты, которую распадки забирают у гор между острыми гребнями; при ней же одни
 * массивы выше соседних. `warp` — на сколько клеток подножие уходит от линии вперёд отрогами и назад
 * заливами. С `spurs` гребни вытянуты поперёк гор во столько раз: отроги спускаются от главного
 * гребня к подножию, между ними лежат кулуары, а массивы сменяют друг друга вдоль гор.
 */
export function applyRange(grid, range) {
  const { height, depth, roughness, wavelength, profile, spurs, seed } = range;
  const ridges = ridged(seed);
  const massifs = fractal(seed + 2, 2);
  const placeOf = rangePlace(range);
  grid.each(grid.whole, (index, x, y) => {
    const { reach, along } = placeOf(x, y);
    if (reach <= 0) return;
    const t = Math.min(1, reach / depth);
    const rise = profile === "middle" ? smoothstep(0, 1, t) : 1 - (1 - t) ** 3;
    const [u, v] = spurs ? [along, reach / spurs] : [x, y];
    // Квадрат хребтов оставляет высоким только сам гребень: склоны к распадкам вогнутые, вершины острые.
    const crest = ridges(u / wavelength, v / wavelength) ** 2;
    const massif = 1 + MASSIF_SPREAD * roughness * spread(massifs(u / (wavelength * 3), v / (wavelength * 3)));
    grid.h[index] += height * rise * (1 - roughness + roughness * crest) * massif;
  });
}

/**
 * Холм (или яма при отрицательной `height`) радиусом `radius` с мягким краем. `irregular` — доля
 * радиуса, на которую край холма уходит внутрь и наружу: холм не круглый, а неровный.
 */
export function applyHill(grid, { at, radius, height, irregular, seed }) {
  const wobble = fractal(seed, 3);
  const reach = radius * (1 + irregular);
  const box = { minX: at[0] - reach, minY: at[1] - reach, maxX: at[0] + reach, maxY: at[1] + reach };
  grid.each(box, (index, x, y) => {
    // Край холма зависит только от направления из середины — шум на окружности вокруг неё: по
    // любому лучу высота спадает до нуля один раз, без бугров и колец за краем.
    const angle = Math.atan2(y - at[1], x - at[0]);
    const local = radius * (1 + irregular * spread(wobble(Math.cos(angle) * 1.5, Math.sin(angle) * 1.5)));
    const d = Math.hypot(x - at[0], y - at[1]);
    if (d < local) grid.h[index] += height * (1 - smoothstep(0, local, d));
  });
}

/**
 * Ровная площадка в многоугольнике `area`: внутри высота `height` (без неё — средняя земля под
 * площадкой), на кромке шириной `rim` площадка плавно сходит к окружающей земле.
 */
export function applyPad(grid, { area, height, rim }) {
  let level = height;
  if (level === undefined) {
    let sum = 0;
    let count = 0;
    grid.each(area.box, (index, x, y) => {
      if (area.contains([x, y])) {
        sum += grid.h[index];
        count += 1;
      }
    });
    level = count > 0 ? sum / count : grid.heightAt(area.points[0][0], area.points[0][1]);
  }
  grid.each(grown(area.box, rim), (index, x, y) => {
    const d = area.signedDistance([x, y]);
    if (d <= 0) grid.h[index] = level;
    else if (d < rim) grid.h[index] = level + (grid.h[index] - level) * smoothstep(0, rim, d);
  });
}

/**
 * Русло по линии `line`: дно шириной `width` на высоте `bottom` (или от первого числа пары в начале
 * линии до второго в конце), берег шириной `bank` сходит к земле. Земля только опускается.
 */
export function applyChannel(grid, { line, width, bottom, bank }) {
  const half = width / 2;
  const [from, to] = Array.isArray(bottom) ? bottom : [bottom, bottom];
  grid.each(grown(line.box, half + bank), (index, x, y) => {
    const near = line.nearest([x, y]);
    if (near.distance > half + bank) return;
    const floor = from + ((to - from) * near.s) / line.length;
    const h = grid.h[index];
    const target = near.distance <= half ? floor : floor + (h - floor) * smoothstep(0, bank, near.distance - half);
    grid.h[index] = Math.min(h, target);
  });
}

/**
 * Сглаживание: `passes` раз каждая точка становится средним точек сетки в квадрате полклетки в
 * каждую сторону от неё — при любой густоте сетки одна и та же мера; с `area` — только внутри, с
 * `except` — кроме гор этой операции `range`: острые гребни и уступы остаются острыми.
 */
export function applySmooth(grid, { area, except, passes }) {
  const reach = Math.max(1, Math.round(grid.density / 2));
  const mountains = except && rangeReach(except);
  const skip = new Uint8Array(grid.cols * grid.rows);
  if (mountains) grid.each(grid.whole, (index, x, y) => (skip[index] = mountains(x, y) > 0 ? 1 : 0));
  for (let pass = 0; pass < passes; pass++) {
    const was = Float64Array.from(grid.h);
    grid.each(area ? area.box : grid.whole, (index, x, y) => {
      if (skip[index] || (area && !area.contains([x, y]))) return;
      const row = Math.round(y * grid.density);
      const col = Math.round(x * grid.density);
      let sum = 0;
      let count = 0;
      for (let r = Math.max(0, row - reach); r <= Math.min(grid.rows - 1, row + reach); r++) {
        for (let c = Math.max(0, col - reach); c <= Math.min(grid.cols - 1, col + reach); c++) {
          sum += was[r * grid.cols + c];
          count += 1;
        }
      }
      grid.h[index] = sum / count;
    });
  }
}
