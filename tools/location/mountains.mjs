// Горы по образцу генераторов рельефа (Gaea, World Machine, Houdini): поверх общей массы гор
// (`range`) — пласты с уступами, глыбы, трещины, размыв водой и осыпи. Каждая операция действует
// только в горах своего `range`, а пласты, глыбы и трещины — ещё и только на крутом: на ровной
// площадке уступов нет. Размыв и осыпи копят карты сетки (`Grid.map`): где текла вода (`flow`), что
// она унесла (`wear`) и куда положила (`deposit`), где лежит сыпучий камень (`loose`) — по ним
// раскладываются покрытия и цвет гор. Все размеры — в клетках, поэтому вид не зависит от густоты сетки.

import { cells, fractal } from "./noise.mjs";
import { random } from "./random.mjs";
import { rangeReach, smoothstep, spread } from "./terrain.mjs";

/** Клеток за подножием, на которых операция гор набирает полную силу. */
const ZONE_FADE = 3;
const DEGREE = Math.PI / 180;

const zones = new WeakMap();

/**
 * Сила операции в горах `range` по точкам сетки: 0 перед подножием, 1 дальше `ZONE_FADE` клеток за
 * ним. Считается один раз на сетку и горы: расстояние до линии подножия — самое долгое в операциях.
 */
function zoneOf(grid, range) {
  let byRange = zones.get(grid);
  if (!byRange) zones.set(grid, (byRange = new Map()));
  if (!byRange.has(range)) {
    const reach = rangeReach(range);
    const zone = new Float32Array(grid.cols * grid.rows);
    grid.each(grid.whole, (index, x, y) => {
      zone[index] = smoothstep(0, ZONE_FADE, reach(x, y));
    });
    byRange.set(range, zone);
  }
  return byRange.get(range);
}

/** Сила в месте `(x, y)` — по ближайшей точке сетки. */
function zoneAt(grid, zone, x, y) {
  const col = Math.min(grid.cols - 1, Math.max(0, Math.round(x * grid.density)));
  const row = Math.min(grid.rows - 1, Math.max(0, Math.round(y * grid.density)));
  return zone[row * grid.cols + col];
}

/** Доля крутизны: 0 положе `from` градусов, 1 круче `full`. */
function steepness(grid, index, h, from, full) {
  return smoothstep(from, full, Math.atan(grid.slopeTan(index, h)) / DEGREE);
}

/** Самая слабая слоистость плиты — доля от `strength`: у других плит она больше, до полной. */
const PLATE_LAYERING = 0.35;

/**
 * Пласты: крутой склон ломается на ступени высотой `step` — ровная полка, затем подъём на доле
 * `riser` ступени. Пласты наклонены на `tilt` градусов к стороне `azimuth` (градусы от оси x к оси
 * y), их границы гнёт шум на `warp` клеток волной `wavelength`, а плиты шириной `plates` клеток
 * сдвинуты по высоте друг против друга и слоисты по-разному: пласт не тянется ровной линией через
 * весь хребет. `strength` — насколько ступень доходит до полной.
 */
export function applyStrata(grid, { range, step, riser, tilt, azimuth, warp, wavelength, plates, strength, from, full, seed }) {
  const zone = zoneOf(grid, range);
  const bend = fractal(seed, 3);
  const plate = cells(seed + 1);
  const lean = Math.tan(tilt * DEGREE);
  const [dx, dy] = [Math.cos(azimuth * DEGREE) * lean, Math.sin(azimuth * DEGREE) * lean];
  const was = Float64Array.from(grid.h);
  grid.each(grid.whole, (index, x, y) => {
    const h = was[index];
    const { id } = plate(x / plates, y / plates);
    // Плиты слоисты по-разному: одни стены в частых уступах, другие почти гладкие.
    const layered = PLATE_LAYERING + (1 - PLATE_LAYERING) * ((id * 7.31) % 1);
    const weight = zone[index] * steepness(grid, index, was, from, full) * strength * layered;
    if (weight <= 0) return;
    const shift = dx * x + dy * y + warp * spread(bend(x / wavelength, y / wavelength)) + id * step;
    const t = (h + shift) / step;
    const level = Math.floor(t);
    const terraced = (level + smoothstep(1 - riser, 1, t - level)) * step - shift;
    grid.h[index] = h + (terraced - h) * weight;
  });
}

/**
 * Глыбы: крутой склон раскалывается на куски шириной около `size` клеток, каждый выдвинут или утоплен
 * на случайную высоту до `amount` клеток — между ними остаются сколы.
 */
export function applyBlocks(grid, { range, size, amount, from, full, seed }) {
  const zone = zoneOf(grid, range);
  const cell = cells(seed);
  const was = Float64Array.from(grid.h);
  grid.each(grid.whole, (index, x, y) => {
    const weight = zone[index] * steepness(grid, index, was, from, full);
    if (weight <= 0) return;
    grid.h[index] += amount * (cell(x / size, y / size).id * 2 - 1) * weight;
  });
}

/**
 * Трещины: по границам кусков шириной около `size` клеток крутой склон прорезан желобами шириной
 * `width` и глубиной `depth` клеток.
 */
export function applyCracks(grid, { range, size, width, depth, from, full, seed }) {
  const zone = zoneOf(grid, range);
  const cell = cells(seed);
  const was = Float64Array.from(grid.h);
  grid.each(grid.whole, (index, x, y) => {
    const weight = zone[index] * steepness(grid, index, was, from, full);
    if (weight <= 0) return;
    const distance = cell(x / size, y / size).border * size;
    const groove = Math.max(0, 1 - distance / (width / 2));
    grid.h[index] -= depth * groove * groove * weight;
  });
}

/** Прямоугольник, в котором лежат горы `range`, — чтобы капли падали только на них. */
function zoneBox(grid, zone) {
  const box = { minX: Infinity, minY: Infinity, maxX: -Infinity, maxY: -Infinity };
  grid.each(grid.whole, (index, x, y) => {
    if (zone[index] <= 0) return;
    box.minX = Math.min(box.minX, x);
    box.minY = Math.min(box.minY, y);
    box.maxX = Math.max(box.maxX, x);
    box.maxY = Math.max(box.maxY, y);
  });
  return box;
}

/** Высота и наклон по x и y в месте `(x, y)` — по квадрату сетки, гладко, без излома на диагонали. */
function sample(grid, x, y) {
  const d = grid.density;
  const gx = Math.min(grid.cols - 1.000001, Math.max(0, x * d));
  const gy = Math.min(grid.rows - 1.000001, Math.max(0, y * d));
  const col = Math.floor(gx);
  const row = Math.floor(gy);
  const u = gx - col;
  const v = gy - row;
  const i = row * grid.cols + col;
  const [a, b, c, e] = [grid.h[i], grid.h[i + 1], grid.h[i + grid.cols], grid.h[i + grid.cols + 1]];
  return {
    h: a * (1 - u) * (1 - v) + b * u * (1 - v) + c * (1 - u) * v + e * u * v,
    gx: ((b - a) * (1 - v) + (e - c) * v) * d,
    gy: ((c - a) * (1 - u) + (e - b) * u) * d,
    col,
    row,
    u,
    v,
  };
}

/**
 * Размыв водой — капли по образцу Hans Theobald Beyer («Implementation of a method for hydraulic
 * erosion», 2015): `drops` капель на клетку гор падают в случайные места и катятся вниз с инерцией
 * `inertia`, шагами по `STEP_LENGTH` клетки, не дольше `lifetime` шагов. Капля уносит тем больше
 * камня, чем она быстрее, полнее и чем круче спуск (`capacity`); лишнее кладёт (`deposit`), недобор
 * размывает кистью радиусом `radius` клеток (`erode`); вода испаряется на долю `evaporate` за шаг.
 * Вышедшая из гор капля только кладёт то, что несла: у подножия растут конусы выноса.
 */
const STEP_LENGTH = 0.25;
const MIN_DROP = 0.01; // клеток спуска за шаг: даже на ровном капля что-то несёт

export function applyErosion(grid, { range, drops, lifetime, inertia, capacity, erode, deposit, evaporate, gravity, radius, seed }) {
  const zone = zoneOf(grid, range);
  const next = random(seed);
  const flow = grid.map("flow");
  const wear = grid.map("wear");
  const laid = grid.map("deposit");
  const loose = grid.map("loose");
  const d = grid.density;
  const reach = Math.ceil(radius * d);
  const brush = [];
  for (let dr = -reach; dr <= reach; dr++) {
    for (let dc = -reach; dc <= reach; dc++) {
      const w = Math.max(0, radius - Math.hypot(dc, dr) / d);
      if (w > 0) brush.push([dc, dr, w]);
    }
  }
  const box = zoneBox(grid, zone);
  const area = (box.maxX - box.minX) * (box.maxY - box.minY);
  const count = Math.round(drops * area);

  const put = (s, amount) => {
    const i = s.row * grid.cols + s.col;
    const parts = [
      [i, (1 - s.u) * (1 - s.v)],
      [i + 1, s.u * (1 - s.v)],
      [i + grid.cols, (1 - s.u) * s.v],
      [i + grid.cols + 1, s.u * s.v],
    ];
    for (const [k, w] of parts) {
      grid.h[k] += amount * w;
      laid[k] += amount * w;
      loose[k] += amount * w;
    }
  };
  const carve = (s, amount) => {
    const col = Math.round(s.col + s.u);
    const row = Math.round(s.row + s.v);
    let total = 0;
    const hits = [];
    for (const [dc, dr, w] of brush) {
      const c = col + dc;
      const r = row + dr;
      if (c < 0 || r < 0 || c >= grid.cols || r >= grid.rows) continue;
      hits.push([r * grid.cols + c, w]);
      total += w;
    }
    for (const [k, w] of hits) {
      const take = (amount * w) / total;
      grid.h[k] -= take;
      wear[k] += take;
      loose[k] = Math.max(0, loose[k] - take);
    }
  };

  for (let n = 0; n < count; n++) {
    let x = box.minX + next() * (box.maxX - box.minX);
    let y = box.minY + next() * (box.maxY - box.minY);
    if (next() > zoneAt(grid, zone, x, y)) continue;
    let [dirX, dirY] = [0, 0];
    let speed = 1;
    let water = 1;
    let sediment = 0;
    for (let life = 0; life < lifetime; life++) {
      const here = sample(grid, x, y);
      dirX = dirX * inertia - here.gx * (1 - inertia);
      dirY = dirY * inertia - here.gy * (1 - inertia);
      const length = Math.hypot(dirX, dirY);
      if (length < 1e-9) {
        const angle = next() * Math.PI * 2;
        [dirX, dirY] = [Math.cos(angle), Math.sin(angle)];
      } else {
        [dirX, dirY] = [dirX / length, dirY / length];
      }
      const nx = x + dirX * STEP_LENGTH;
      const ny = y + dirY * STEP_LENGTH;
      if (nx < 0 || ny < 0 || nx >= grid.width || ny >= grid.height) break;
      const fall = sample(grid, nx, ny).h - here.h;
      flow[Math.round(here.row + here.v) * grid.cols + Math.round(here.col + here.u)] += water;
      // Вне гор капля только кладёт: у подножия растут конусы выноса, а не промоины.
      const outside = zoneAt(grid, zone, nx, ny) <= 0;
      const room = Math.max(-fall, MIN_DROP) * speed * water * capacity * (outside ? 0 : 1);
      if (sediment > room || fall > 0 || outside) {
        const amount = fall > 0 ? Math.min(fall, sediment) : (sediment - room) * deposit;
        sediment -= amount;
        put(here, amount);
      } else {
        const amount = Math.min((room - sediment) * erode, -fall);
        carve(here, amount);
        sediment += amount;
      }
      speed = Math.sqrt(Math.max(0, speed * speed - fall * gravity));
      water *= 1 - evaporate;
      x = nx;
      y = ny;
    }
  }
}

/**
 * Осыпи: крутые стены круче `cliff` градусов крошатся — за шаг доля `weathering` лишней крутизны
 * становится сыпучим камнем, — а сыпучий камень (он же — вынесенный водой) сползает вниз, пока склон
 * под ним круче `repose` градусов. Так под стенами растут конусы и шлейфы осыпей. `iterations` шагов.
 */
export function applyTalus(grid, { range, repose, cliff, weathering, iterations }) {
  const zone = zoneOf(grid, range);
  const loose = grid.map("loose");
  const d = grid.density;
  const settle = Math.tan(repose * DEGREE);
  const crumble = Math.tan(cliff * DEGREE);
  const neighbours = [];
  for (let dr = -1; dr <= 1; dr++) {
    for (let dc = -1; dc <= 1; dc++) {
      if (dr !== 0 || dc !== 0) neighbours.push([dc, dr, Math.hypot(dc, dr) / d]);
    }
  }
  const moved = new Float64Array(grid.cols * grid.rows);
  for (let pass = 0; pass < iterations; pass++) {
    moved.fill(0);
    for (let row = 0; row < grid.rows; row++) {
      for (let col = 0; col < grid.cols; col++) {
        const index = row * grid.cols + col;
        // Сыпучий камень сползает и за подножием, иначе он копился бы стенкой на границе гор.
        if (zone[index] <= 0 && loose[index] <= 0) continue;
        const h = grid.h[index];
        let steepest = 0;
        let total = 0;
        const drops = [];
        for (const [dc, dr, run] of neighbours) {
          const c = col + dc;
          const r = row + dr;
          if (c < 0 || r < 0 || c >= grid.cols || r >= grid.rows) continue;
          const k = r * grid.cols + c;
          const tan = (h - grid.h[k]) / run;
          steepest = Math.max(steepest, tan);
          if (tan > settle) {
            const excess = (tan - settle) * run;
            drops.push([k, excess]);
            total += excess;
          }
        }
        if (steepest > crumble) loose[index] += (zone[index] * weathering * (steepest - crumble)) / d;
        if (total <= 0 || loose[index] <= 0) continue;
        // Сползает не больше половины перепада сверх покоя и не больше сыпучего слоя.
        const amount = Math.min(loose[index], (total / drops.length) * 0.5);
        moved[index] -= amount;
        for (const [k, excess] of drops) moved[k] += (amount * excess) / total;
      }
    }
    for (let index = 0; index < moved.length; index++) {
      if (moved[index] === 0) continue;
      grid.h[index] += moved[index];
      loose[index] = Math.max(0, loose[index] + moved[index]);
    }
  }
}
