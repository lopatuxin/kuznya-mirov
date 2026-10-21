// Генератор голой карты края: пологие холмы, море вдоль северного края и реки с притоками, впадающие в море.
// Пишет рельеф `terrain.json` с водой — морем, реками и озёрами записями движка, — карту цвета земли
// `terrain/tint.png` и размер сцены в `game.json` папки игры `out`; одно зерно `seed` — та же карта. Гладь реки
// поднимается от устья к истоку на `fall_m`, на перекате падает ступенькой, в озере стоит ровно; долина
// повторяет гладь своей реки, поэтому пойма везде выше воды.
// Запуск: node tools/city/land.mjs art/transport/land.json

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { random, round } from "./geo.mjs";
import { gameText, hex } from "./osm2scene.mjs";
import { encodePng } from "./png.mjs";

const TINT_SIZE = 1024;
const RIVER_STEP_M = 20;     // шаг, которым прокладывается русло
const RIVER_GAP_M = 450;     // ближе этого реки не подходят друг к другу: долины сливались бы
const EDGE_MARGIN_M = 300;   // от края карты до русла
const BANK_M = 12;           // ширина берега от уреза до поймы
const BEACH_M = 80;          // ширина пляжа у моря
const NEAR_M = 700;          // полоса за поймой, где расстояние до неё считается точно
const TRIBUTARY_TURN = 0.95; // угол притока к реке у слияния, радианы
const ATTEMPTS = 40;
const FALL_POWER = 1.3;      // гладь круче к истоку: высота над устьем — доля длины в этой степени
const STRIP_MARGIN_M = 6;    // русло реки для движка шире берегов на столько с каждой стороны
const EXACT_NEAR_M = 0;      // в пойме долина стоит на глади своей реки
const BLEND_NEAR_M = 500;    // дальше этого от поймы — на сглаженной глади всех рек и моря
const LAKE_SHORE_M = 40;     // берег озера от уреза до поймы
const ESTUARY_M = 400;       // на столько выше впадения в море или озеро река мелеет до глубины их отмели
const LAKE_SHELF = 0.35;     // на такой доле радиуса от уреза озеро набирает полную глубину: узкая мелкая кайма
const TURN_BACK = 0.5;       // шаг русла, повернувший круче 60° от прежнего, обрывает его: так русла не поворачивают
const VALLEY_SMOOTH_M = 300; // долина под перекатом поднимается к его верху плавно на столько вниз по течению

/** Голая карта по настройкам: высоты точек в метрах над водой, расстояния до воды и русла рек. */
export function buildLand(config) {
  const [widthM, heightM] = config.size_m;
  const spacing = config.cell_m / config.points_per_cell;
  const cols = Math.round(widthM / spacing) + 1;
  const rows = Math.round(heightM / spacing) + 1;
  const noise = perlin(config.seed);
  const coastWiggle = layer(noise, [2600, 1000, 380], 3.1);
  const coast = (x) => config.sea.width_m + config.sea.coast_wiggle_m * coastWiggle(x, 0);
  const rivers = layRivers(config, noise, coast);
  assignSurfaces(rivers, noise);
  const lakes = rivers.filter((r) => r.lake).map((r) => r.lake);
  const { hills, floodplain, sea } = config;
  // Пойма — полоса ровного луга вдоль реки: шире у больших рек и в низовьях, к истоку сходит на нет.
  const spread = layer(noise, [900, 350], 29.5);
  const widest = Math.max(...config.rivers.map((r) => r.width_m[0]));
  for (const river of rivers) {
    for (const p of river.points) {
      const wide = floodplain.min_m + (floodplain.max_m - floodplain.min_m) * clamp01(0.5 + 0.6 * spread(p.x, p.y));
      const size = 0.45 + 0.55 * Math.min(1, p.width / widest);
      p.flood = Math.max(p.width / 2 + 20, wide * size * (0.3 + 0.7 * (1 - p.k)));
    }
  }

  const n = cols * rows;
  const channel = new Float32Array(n).fill(Infinity); // от уреза ближайшей реки, внутри русла — меньше нуля
  const depth = new Float32Array(n);                  // глубина ближайшей реки
  const near = new Float32Array(n).fill(Infinity);    // от края поймы, точно — в полосе `NEAR_M` за ней
  const surf = new Float32Array(n).fill(NaN);         // гладь ближайшей реки в её ближайшей точке
  const valley = new Float32Array(n).fill(NaN);       // сглаженная вдоль реки гладь, на которой стоит её долина
  const source = new Uint8Array(n);                   // вода и пойма: от них растут склоны
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) if (r * spacing < coast(c * spacing)) source[r * cols + c] = 1;
  }
  for (const river of rivers) rasterRiver(river, { cols, rows, spacing, channel, depth, near, source, surf, valley });
  for (let i = 0; i < n; i++) if (channel[i] <= 0) source[i] = 1;
  for (const lake of lakes) {
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) if (lakeEdge(lake, c * spacing, r * spacing) < LAKE_SHORE_M) source[r * cols + c] = 1;
    }
  }
  // Вдали от рек хватает расстояния по сетке; у самой поймы оно ступенчатое, и склон пошёл бы рябью.
  const far = distanceField(source, cols, rows, spacing);
  const dist = new Float32Array(n);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const i = r * cols + c;
      dist[i] = Math.min(near[i], far[i], Math.max(0, r * spacing - coast(c * spacing)));
    }
  }

  const base = waterBase({ cols, rows, spacing, widthM, rivers, lakes, coast, valley, near });
  const top = layer(noise, [2600, 1100, 450], 7.7);
  const mid = layer(noise, [1500, 650, 300], 41.2);
  const bumps = layer(noise, [420, 160, 70], 13.9);
  const warp = layer(noise, [1200, 500], 55.1);
  const meadow = layer(noise, [500, 160], 21.3);
  const heights = new Float32Array(n);
  for (let r = 0; r < rows; r++) {
    const y = r * spacing;
    for (let c = 0; c < cols; c++) {
      const x = c * spacing;
      const i = r * cols + c;
      const shore = coast(x);
      if (y < shore) {
        // У самого берега дно на уровне моря, как и пляж: высота непрерывна, и кромка воды идёт по берегу, а не лесенкой
        // посередине между точками сетки.
        heights[i] = -Math.min(sea.depth_m, sea.depth_m * ((shore - y) / sea.shelf_m));
        continue;
      }
      const inland = clamp01((y - shore) / (heightM - shore));
      const peak = (hills.min_m + (hills.max_m - hills.min_m) * clamp01(0.5 + 0.6 * top(x, y))) *
        (1 - hills.inland_rise + hills.inland_rise * inland);
      // Кривой масштаб расстояния ломает прямые гребни там, где сходятся склоны двух долин.
      const d = dist[i] * (1 + 0.35 * warp(x, y));
      const away = smoothstep(0, hills.flat_near_water_m, d);
      // Склон долины — плавная S-образная кривая: от поймы земля поднимается без излома, круче всего на полпути
      // к верху, и долина не обведена узкой крутой бровкой.
      const rise = 1 - Math.exp(-((d / hills.wall_m) ** 2));
      let h = floodplain.height_m + 0.35 * meadow(x, y) + peak * rise +
        (hills.mid_m * mid(x, y) + hills.bumps_m * bumps(x, y)) * away;
      h = Math.max(h, 0.4);
      // Пляж: к морю земля сходит полого, и кромка воды идёт ровной линией, а не лесенкой по точкам сетки.
      const beach = y - shore;
      if (beach < BEACH_M) h = 0.02 * beach + (h - 0.02 * beach) * smoothstep(0, BEACH_M, beach);
      // Долина стоит на глади своей реки или озера, сглаженной вдоль реки; русло и берег — на самой глади, поэтому
      // под перекатом река идёт в теснине, а не ступенькой поперёк всей долины.
      let z = base[i] + h;
      const edge = channel[i];
      if (edge < BANK_M) {
        const brim = surf[i] - 0.3;
        // У моря и озера река мелеет до глубины их отмели, как у настоящего устья с наносами: иначе тёмная глубокая
        // лента упиралась бы в светлую отмель, и место впадения было бы видно резкой границей цвета.
        const still = Math.min(y - shore, ...lakes.map((lake) => lakeEdge(lake, x, y)));
        const shoal = smoothstep(0, ESTUARY_M, Math.max(0, still));
        if (edge < 0) z = brim - depth[i] * shoal * Math.min(1, -edge / 14);
        else z = brim + (z - brim) * smoothstep(0, BANK_M, edge);
      }
      // Чаша озера: дно глубже к середине, берег от уреза поднимается к пойме.
      for (const lake of lakes) {
        const out = lakeEdge(lake, x, y);
        const brim = lake.level - 0.3;
        if (out < 0) z = Math.min(z, brim - lake.depth * smoothstep(0, LAKE_SHELF * lake.radius, -out));
        else if (out < LAKE_SHORE_M) z = Math.min(z, brim + (z - brim) * smoothstep(0, LAKE_SHORE_M, out));
      }
      heights[i] = z;
    }
  }
  return { cols, rows, spacing, widthM, heightM, heights, base, channel, rivers, lakes, coast };
}

/**
 * Гладь по течению: от устья — или от глади реки у слияния — вверх к истоку на `fall_m`, круче к истоку; перекат —
 * ступенька `drop_m` на длине `length_m`; в озере гладь ровная. Озеро — чаша вокруг точки реки `lake.at`.
 */
function assignSurfaces(rivers, noise) {
  for (const river of rivers) {
    const spec = river.spec;
    const start = river.parent ? river.parent.points[river.joinIndex].s : 0;
    const total = Math.max(river.length, 1);
    for (const p of river.points) {
      let s = start + (spec.fall_m ?? 0) * p.k ** FALL_POWER;
      for (const rapid of spec.rapids ?? []) {
        // Перекат не короче шага русла: ступенька нулевой длины дала бы деление на ноль.
        const half = Math.max(rapid.length_m, RIVER_STEP_M) / 2 / total;
        s += rapid.drop_m * smoothstep(rapid.at - half, rapid.at + half, p.k);
      }
      p.s = s;
    }
    if (spec.lake) river.lake = placeLake(river, spec.lake, noise);
    smoothValley(river);
  }
}

/** Гладь, на которой стоит долина: средняя по реке на `VALLEY_SMOOTH_M` в обе стороны, но не ниже самой глади. */
function smoothValley(river) {
  const pts = river.points;
  const reach = Math.round(VALLEY_SMOOTH_M / RIVER_STEP_M);
  for (let k = 0; k < pts.length; k++) {
    let sum = 0, count = 0;
    for (let j = Math.max(0, k - reach); j <= Math.min(pts.length - 1, k + reach); j++) {
      sum += pts[j].s;
      count++;
    }
    pts[k].v = Math.max(pts[k].s, sum / count);
  }
}

/** Озеро на реке: гладь выравнивается по точке посередине на участке русла внутри чаши. */
function placeLake(river, spec, noise) {
  const pts = river.points;
  const mid = pts.reduce((best, p) => (Math.abs(p.k - spec.at) < Math.abs(best.k - spec.at) ? p : best), pts[0]);
  const at = pts.indexOf(mid);
  const inside = (p) => Math.hypot(p.x - mid.x, p.y - mid.y) <= spec.radius_m;
  let lo = at, hi = at;
  while (lo > 0 && inside(pts[lo - 1])) lo--;
  while (hi < pts.length - 1 && inside(pts[hi + 1])) hi++;
  for (let k = lo; k <= hi; k++) pts[k].s = mid.s;
  const next = pts[Math.min(at + 1, pts.length - 1)], prev = pts[Math.max(at - 1, 0)];
  const len = Math.hypot(next.x - prev.x, next.y - prev.y) || 1;
  const side = { x: -(next.y - prev.y) / len, y: (next.x - prev.x) / len };
  const shore = (angle) => spec.radius_m * (0.78 + 0.16 * noise(Math.cos(angle) * 1.3 + 71.3, Math.sin(angle) * 1.3 + 5.9));
  return {
    x: mid.x, y: mid.y, level: mid.s, radius: spec.radius_m, depth: spec.depth_m, shore,
    // Точки русла в чаше: `outlet` — ближняя к устью, `inlet` — ближняя к истоку.
    outlet: lo, inlet: hi,
    // Точка озера для движка — в стороне от русла: в русле главнее река.
    at: { x: mid.x + side.x * 0.4 * spec.radius_m, y: mid.y + side.y * 0.4 * spec.radius_m },
  };
}

/** Насколько точка дальше уреза озера, в метрах; внутри озера — меньше нуля. */
function lakeEdge(lake, x, y) {
  const dx = x - lake.x, dy = y - lake.y;
  return Math.hypot(dx, dy) - lake.shore(Math.atan2(dy, dx));
}

/**
 * Гладь, на которой стоит долина в каждой точке: в пойме — сглаженная вдоль реки гладь её ближайшей точки, вдали —
 * гладь рек, озёр и моря, сглаженная по расстоянию. Вокруг озера ближайшая точка реки — на участке, выровненном по
 * озеру, а сглаженная гладь не ниже самой глади, поэтому берег чаши не ниже уровня озера.
 */
function waterBase({ cols, rows, spacing, widthM, rivers, lakes, coast, valley, near }) {
  const samples = [];
  for (const river of rivers) for (let k = 0; k < river.points.length; k += 3) samples.push(river.points[k].x, river.points[k].y, river.points[k].v);
  for (let x = 0; x <= widthM; x += 100) samples.push(x, coast(x), 0);
  for (const lake of lakes) samples.push(lake.x, lake.y, lake.level);
  const base = new Float32Array(cols * rows);
  for (let r = 0; r < rows; r++) {
    const y = r * spacing;
    for (let c = 0; c < cols; c++) {
      const x = c * spacing;
      const i = r * cols + c;
      const far = smoothstep(EXACT_NEAR_M, BLEND_NEAR_M, near[i]);
      const exact = Number.isFinite(valley[i]);
      let b = exact ? valley[i] : 0;
      if (!exact || far > 0) {
        let sw = 0, sv = 0;
        for (let j = 0; j < samples.length; j += 3) {
          const d2 = (x - samples[j]) ** 2 + (y - samples[j + 1]) ** 2 + 1;
          const w = 1 / (d2 * Math.sqrt(d2));
          sw += w;
          sv += w * samples[j + 2];
        }
        const smooth = sv / sw;
        b = exact ? b + (smooth - b) * far : smooth;
      }
      base[i] = b;
    }
  }
  return base;
}

/** Русла рек и притоков: от устья у моря вверх по течению, извилисто, не ближе `RIVER_GAP_M` друг к другу. */
function layRivers(config, noise, coast) {
  const [widthM, heightM] = config.size_m;
  // Уже полутора шагов сетки русло рвётся на лужи: между точками высот вода не проходит.
  const narrowest = (1.5 * config.cell_m) / config.points_per_cell;
  for (let attempt = 0; attempt < ATTEMPTS; attempt++) {
    const laid = [];
    let ok = true;
    for (const [index, spec] of config.rivers.entries()) {
      const x = spec.mouth * widthM;
      const start = { x, y: coast(x) - 150 };
      const main = traceRiver({
        start, heading: Math.PI / 2, length: spec.length * heightM, widths: spec.width_m, depth: spec.depth_m,
        estuary: true, wiggle: wiggle(noise, 101 * index + 13 * attempt), laid, parent: null, coast, widthM, heightM, narrowest,
      });
      if (main.length < 0.5 * spec.length * heightM) {
        ok = false;
        break;
      }
      main.spec = spec;
      laid.push(main);
      for (const [t, trib] of spec.tributaries.entries()) {
        const at = main.points[Math.round(trib.at * (main.points.length - 1))];
        const next = main.points[Math.min(main.points.length - 1, main.points.indexOf(at) + 1)];
        const along = Math.atan2(next.y - at.y, next.x - at.x);
        const branch = traceRiver({
          start: at, heading: along + trib.side * TRIBUTARY_TURN, length: trib.length * heightM, widths: trib.width_m,
          depth: spec.depth_m * 0.7, estuary: false, wiggle: wiggle(noise, 101 * index + 37 * t + 13 * attempt + 50),
          laid, parent: main, coast, widthM, heightM, narrowest,
        });
        if (branch.length < 0.4 * trib.length * heightM) {
          ok = false;
          break;
        }
        branch.spec = trib;
        branch.parent = main;
        branch.joinIndex = main.points.indexOf(at);
        laid.push(branch);
      }
      if (!ok) break;
    }
    if (ok) return laid;
  }
  throw new Error(`реки не легли за ${ATTEMPTS} попыток: уменьши длину или число рек`);
}

/**
 * Одно русло от `start` в сторону `heading`: направление виляет по шуму, у краёв карты заворачивает внутрь.
 * Обрывается у края, у моря (приток), там, где подошло бы к другому руслу ближе `RIVER_GAP_M`, и там, где шаг
 * повернул бы круче `TURN_BACK`: у края карты притяжение внутрь и извив уравновешиваются, и русло топталось бы на месте.
 * `run` точки — путь по руслу от начала.
 */
function traceRiver({ start, heading, length, widths, depth, estuary, wiggle, laid, parent, coast, widthM, heightM, narrowest }) {
  const points = [{ x: start.x, y: start.y, run: 0 }];
  let { x, y } = start;
  let s = 0;
  let back = null;
  const bend = heading + 0.3 * wiggle(-7.3);
  while (s < length) {
    // Три размера излучин: большие повороты долины, меандры и мелкие изгибы русла.
    const a = bend + 1.1 * wiggle(s / 2500) + 1.0 * wiggle(s / 800 + 20) + 0.5 * wiggle(s / 260 + 40);
    let dx = Math.cos(a), dy = Math.sin(a);
    const pull = (v, lo, hi) => (v < lo ? (lo - v) / EDGE_MARGIN_M : v > hi ? (hi - v) / EDGE_MARGIN_M : 0);
    dx += 1.5 * pull(x, 2 * EDGE_MARGIN_M, widthM - 2 * EDGE_MARGIN_M);
    dy += 1.5 * pull(y, 0, heightM - 2 * EDGE_MARGIN_M);
    const len = Math.hypot(dx, dy);
    if (back && (dx / len) * back.x + (dy / len) * back.y < TURN_BACK) break;
    back = { x: dx / len, y: dy / len };
    x += (dx / len) * RIVER_STEP_M;
    y += (dy / len) * RIVER_STEP_M;
    s += RIVER_STEP_M;
    if (x < EDGE_MARGIN_M || x > widthM - EDGE_MARGIN_M || y > heightM - EDGE_MARGIN_M) break;
    if (y < coast(x) + (parent ? 400 : 0) && s > 200) break;
    if (tooClose(x, y, laid, parent, s)) break;
    points.push({ x, y, run: s });
  }
  const total = points[points.length - 1].run;
  for (const p of points) {
    p.k = total > 0 ? p.run / total : 0;
    const spring = p.k > 0.85 ? 1 - 0.7 * ((p.k - 0.85) / 0.15) : 1;
    const width = (widths[0] + (widths[1] - widths[0]) * p.k) * spring * (estuary ? 1 + 1.6 * Math.exp(-p.run / 350) : 1);
    p.width = Math.max(narrowest, width);
  }
  return { points, length: total, depth, main: parent === null };
}

function tooClose(x, y, laid, parent, s) {
  for (const river of laid) {
    // Приток у слияния касается своей реки — её не мерим, пока он не отошёл на просвет между реками.
    if (river === parent && s < 2 * RIVER_GAP_M) continue;
    for (const p of river.points) if (Math.hypot(p.x - x, p.y - y) < RIVER_GAP_M) return true;
  }
  return false;
}

/** Русло в поля карты: расстояние от уреза и от края поймы, глубина, пойма — источник склонов. */
function rasterRiver(river, { cols, rows, spacing, channel, depth, near, source, surf, valley }) {
  const pts = river.points;
  for (let k = 0; k + 1 < pts.length; k++) {
    const a = pts[k], b = pts[k + 1];
    const wa = a.width / 2, wb = b.width / 2;
    const fa = a.flood, fb = b.flood;
    const reach = Math.max(fa, fb) + NEAR_M;
    const c0 = Math.max(0, Math.floor((Math.min(a.x, b.x) - reach) / spacing));
    const c1 = Math.min(cols - 1, Math.ceil((Math.max(a.x, b.x) + reach) / spacing));
    const r0 = Math.max(0, Math.floor((Math.min(a.y, b.y) - reach) / spacing));
    const r1 = Math.min(rows - 1, Math.ceil((Math.max(a.y, b.y) + reach) / spacing));
    const ux = b.x - a.x, uy = b.y - a.y, uu = ux * ux + uy * uy;
    for (let r = r0; r <= r1; r++) {
      for (let c = c0; c <= c1; c++) {
        const px = c * spacing - a.x, py = r * spacing - a.y;
        const t = uu > 0 ? clamp01((px * ux + py * uy) / uu) : 0;
        const d = Math.hypot(px - t * ux, py - t * uy);
        const i = r * cols + c;
        const edge = d - (wa + (wb - wa) * t);
        if (edge < channel[i]) {
          channel[i] = edge;
          depth[i] = river.depth;
          surf[i] = a.s + (b.s - a.s) * t;
          valley[i] = a.v + (b.v - a.v) * t;
        }
        const out = d - (fa + (fb - fa) * t);
        if (out <= 0) source[i] = 1;
        if (out < near[i]) near[i] = Math.max(0, out);
      }
    }
  }
}

/** Рельеф одной строкой высот на строку файла, в клетках; карта цвета земли. */
function landFiles(land, config) {
  const heights = [];
  for (let r = 0; r < land.rows; r++) {
    const row = new Array(land.cols);
    for (let c = 0; c < land.cols; c++) row[c] = round(land.heights[r * land.cols + c] / config.cell_m, 4);
    heights.push(row);
  }
  return { terrain: terrainFileText(landWater(land, config), "terrain/tint.png", heights), tint: tintImage(land, config) };
}

/** Запись воды для движка в клетках: море на нуле, озёра, реки от истока к устью с шириной русла. */
export function landWater(land, config) {
  const cells = (m) => round(m / config.cell_m, 4);
  // Русло для движка — от поймы до поймы: берег внутри него, вода не упрётся в край полосы.
  const strip = (p) => p.width + 2 * (BANK_M + STRIP_MARGIN_M);
  return {
    color: config.water.color, clarity: config.water.clarity, ripple: config.water.ripple, level: 0,
    lakes: land.lakes.map((lake) => ({ at: [cells(lake.at.x), cells(lake.at.y)], level: cells(lake.level) })),
    // Точки русла легли от устья, у притока — от слияния. Русло кончается у берега: в море река не заходит, иначе
    // там рябь бежала бы по течению, а вокруг её гнал бы ветер, и полоса реки была бы видна в море.
    // Река с озером пишется двумя: верхняя кончается в озере, нижняя вытекает из него. Иначе течение тянулось бы
    // полосой через стоячую воду озера.
    rivers: land.rivers.flatMap((river) => {
      const parts = river.lake
        ? [river.points.slice(0, river.lake.outlet + 1), river.points.slice(river.lake.inlet)]
        : [river.points];
      return parts.map((part) => {
        const kept = part.filter((p, k, all) => p.y >= land.coast(p.x) || (k + 1 < all.length && all[k + 1].y >= land.coast(all[k + 1].x)));
        // Гладь отсчитана от точки русла далеко в море; обрезанная у берега река кончается на уровне моря, иначе
        // вокруг устья её вода стояла бы дугой выше моря.
        const reachesSea = kept[0] !== part[0];
        return {
          points: kept.toReversed().map((p, k, all) => [cells(p.x), cells(p.y), reachesSea && k === all.length - 1 ? 0 : cells(p.s), cells(strip(p))]),
        };
      });
    }),
  };
}

/** Файл рельефа: запись воды первой, озёра и реки — по водоёму на строку, высоты — строка сетки на строку. */
function terrainFileText(water, tint, heights) {
  const { lakes, rivers, ...head } = water;
  const parts = [`    ${Object.entries(head).map(([k, v]) => `${JSON.stringify(k)}: ${JSON.stringify(v)}`).join(", ")}`];
  const list = (name, items) => `    ${JSON.stringify(name)}: [\n${items.map((v) => `      ${JSON.stringify(v)}`).join(",\n")}\n    ]`;
  if (lakes.length > 0) parts.push(list("lakes", lakes));
  if (rivers.length > 0) parts.push(list("rivers", rivers));
  const rowsText = heights.map((r) => `    ${JSON.stringify(r)}`).join(",\n");
  return `{\n  "water": {\n${parts.join(",\n")}\n  },\n  "tint": ${JSON.stringify(tint)},\n  "heights": [\n${rowsText}\n  ]\n}\n`;
}

/**
 * Карта цвета: пойма — сочный луг, выше — суше и желтее, склоны — бурее, у воды — песок у моря и ил у рек.
 * Цвета из настроек делятся на фон сцены: движок умножает фон на карту цвета.
 */
function tintImage(land, config) {
  const w = TINT_SIZE;
  const h = Math.round((TINT_SIZE * (land.rows - 1)) / (land.cols - 1));
  const colors = Object.fromEntries(Object.entries(config.colors).map(([k, v]) => [k, hex(v)]));
  const bg = hex(config.background);
  const patches = layer(perlin(config.seed + 1), [900, 300, 90], 0.37);
  const rgba = new Uint8Array(w * h * 4);
  // Вдали от рек расстояние до уреза бесконечно; для плавной выборки его хватает ограничить.
  const channel = land.channel.map((v) => Math.min(v, 1e4));
  const height = (gx, gy) => bilinear(land.heights, land.cols, land.rows, gx, gy);
  for (let py = 0; py < h; py++) {
    for (let px = 0; px < w; px++) {
      // Движок берёт карту цвета по серединам точек картинки, поэтому и цвет считается в середине точки.
      const x = ((px + 0.5) / w) * land.widthM, y = ((py + 0.5) / h) * land.heightM;
      const gx = x / land.spacing, gy = y / land.spacing;
      // Луг и сухие места — по высоте над водой своей долины, а не над морем.
      const z = height(gx, gy) - bilinear(land.base, land.cols, land.rows, gx, gy);
      const slope = Math.hypot(height(gx + 0.5, gy) - height(gx - 0.5, gy), height(gx, gy + 0.5) - height(gx, gy - 0.5)) / land.spacing;
      const p = patches(x, y);
      let col = mix(colors.meadow, colors.upland, smoothstep(2, 22, z));
      col = mix(col, colors.dry, smoothstep(30, 60, z) * clamp01(0.55 + 0.6 * p));
      col = mix(col, colors.slope, smoothstep(0.1, 0.25, slope));
      // Мокрый берег — полоса у самой воды: песок у моря, ил у рек.
      const fromSea = y - land.coast(x);
      const fromRiver = bilinear(channel, land.cols, land.rows, gx, gy);
      col = mix(col, colors.sand, 1 - smoothstep(0, 45, fromSea));
      // Ил реки — только на суше: линия русла заходит в море, и на мелком дне перед устьем ил был бы виден тёмной полосой.
      col = mix(col, colors.silt, (1 - smoothstep(0, 16, fromRiver)) * smoothstep(0, 20, fromSea));
      for (const lake of land.lakes) col = mix(col, colors.silt, 1 - smoothstep(0, 16, lakeEdge(lake, x, y)));
      const shade = 1 + 0.08 * p;
      const o = (py * w + px) * 4;
      for (let ch = 0; ch < 3; ch++) rgba[o + ch] = Math.max(0, Math.min(255, Math.round((128 * col[ch] * shade) / bg[ch])));
      rgba[o + 3] = 255;
    }
  }
  return encodePng(w, h, rgba);
}

/** Значение поля сетки в дробной точке: между четырьмя соседними точками по прямой, за краем — как на краю. */
function bilinear(field, cols, rows, gx, gy) {
  const x = Math.max(0, Math.min(cols - 1, gx)), y = Math.max(0, Math.min(rows - 1, gy));
  const c = Math.min(cols - 2, Math.floor(x)), r = Math.min(rows - 2, Math.floor(y));
  const fx = x - c, fy = y - r;
  const i = r * cols + c;
  return (field[i] * (1 - fx) + field[i + 1] * fx) * (1 - fy) + (field[i + cols] * (1 - fx) + field[i + cols + 1] * fx) * fy;
}

/** Шум Перлина с перемешиванием по зерну: плавные значения примерно от −1 до 1. */
function perlin(seed) {
  const rand = random(seed * 2654435761);
  const p = Array.from({ length: 256 }, (_, i) => i);
  for (let i = 255; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [p[i], p[j]] = [p[j], p[i]];
  }
  const perm = new Uint8Array(512);
  for (let i = 0; i < 512; i++) perm[i] = p[i & 255];
  const gx = Array.from({ length: 8 }, (_, k) => Math.cos((k * Math.PI) / 4));
  const gy = Array.from({ length: 8 }, (_, k) => Math.sin((k * Math.PI) / 4));
  const dot = (hash, x, y) => gx[hash & 7] * x + gy[hash & 7] * y;
  const fade = (t) => t * t * t * (t * (t * 6 - 15) + 10);
  return (x, y) => {
    const x0 = Math.floor(x), y0 = Math.floor(y);
    const fx = x - x0, fy = y - y0;
    const xi = x0 & 255, yi = y0 & 255;
    const a = dot(perm[perm[xi] + yi], fx, fy);
    const b = dot(perm[perm[xi + 1] + yi], fx - 1, fy);
    const c = dot(perm[perm[xi] + yi + 1], fx, fy - 1);
    const d = dot(perm[perm[xi + 1] + yi + 1], fx - 1, fy - 1);
    const u = fade(fx), v = fade(fy);
    return 1.4 * ((a + (b - a) * u) * (1 - v) + (c + (d - c) * u) * v);
  };
}

/** Слой шума из нескольких длин волн в метрах: каждая следующая вдвое слабее; `shift` разводит слои. */
function layer(noise, wavelengths, shift) {
  const norm = wavelengths.reduce((sum, _, i) => sum + 0.5 ** i, 0);
  return (x, y) => {
    let sum = 0;
    for (let i = 0; i < wavelengths.length; i++) sum += 0.5 ** i * noise(x / wavelengths[i] + shift + 17.3 * i, y / wavelengths[i] + 1.7 * shift + 31.1 * i);
    return sum / norm;
  };
}

const wiggle = (noise, shift) => (t) => noise(t + 0.5 * shift, 0.37 * shift + 0.5);

/** Точное расстояние в метрах от каждой точки до ближайшей отмеченной (Felzenszwalb, Huttenlocher). */
export function distanceField(mask, cols, rows, spacing) {
  const INF = 1e20;
  const n = cols * rows;
  const grid = new Float64Array(n);
  for (let i = 0; i < n; i++) grid[i] = mask[i] ? 0 : INF;
  const size = Math.max(cols, rows);
  const f = new Float64Array(size), d = new Float64Array(size), v = new Int32Array(size), z = new Float64Array(size + 1);
  for (let c = 0; c < cols; c++) {
    for (let r = 0; r < rows; r++) f[r] = grid[r * cols + c];
    edt(f, rows, d, v, z);
    for (let r = 0; r < rows; r++) grid[r * cols + c] = d[r];
  }
  const out = new Float32Array(n);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) f[c] = grid[r * cols + c];
    edt(f, cols, d, v, z);
    for (let c = 0; c < cols; c++) out[r * cols + c] = Math.sqrt(d[c]) * spacing;
  }
  return out;
}

function edt(f, n, d, v, z) {
  let k = 0;
  v[0] = 0;
  z[0] = -Infinity;
  z[1] = Infinity;
  for (let q = 1; q < n; q++) {
    let s = (f[q] + q * q - (f[v[k]] + v[k] * v[k])) / (2 * q - 2 * v[k]);
    while (s <= z[k]) {
      k--;
      s = (f[q] + q * q - (f[v[k]] + v[k] * v[k])) / (2 * q - 2 * v[k]);
    }
    k++;
    v[k] = q;
    z[k] = s;
    z[k + 1] = Infinity;
  }
  k = 0;
  for (let q = 0; q < n; q++) {
    while (z[k + 1] < q) k++;
    d[q] = (q - v[k]) ** 2 + f[v[k]];
  }
}

const clamp01 = (t) => Math.max(0, Math.min(1, t));
const smoothstep = (a, b, t) => {
  const k = clamp01((t - a) / (b - a));
  return k * k * (3 - 2 * k);
};
const mix = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);

export async function run(configPath, root = process.cwd()) {
  const config = JSON.parse(await readFile(configPath, "utf8"));
  const land = buildLand(config);
  const files = landFiles(land, config);
  const out = resolve(root, config.out);
  await mkdir(join(out, "terrain"), { recursive: true });
  await writeFile(join(out, "terrain.json"), files.terrain);
  await writeFile(join(out, "terrain", "tint.png"), files.tint);
  // Сцена игры — ровно размер карты в клетках, иначе движок не примет рельеф.
  const gamePath = join(out, "game.json");
  const game = JSON.parse(await readFile(gamePath, "utf8"));
  game.scene.width = (land.cols - 1) / config.points_per_cell;
  game.scene.height = (land.rows - 1) / config.points_per_cell;
  await writeFile(gamePath, `${gameText(game)}\n`);
  return { points: [land.cols, land.rows], rivers: land.rivers.map((r) => Math.round(r.length)) };
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const configPath = process.argv[2];
  if (!configPath) {
    console.error("укажи файл настроек карты: node tools/city/land.mjs art/transport/land.json");
    process.exit(2);
  }
  run(resolve(configPath)).then(
    (stats) => console.log(JSON.stringify(stats)),
    (error) => {
      console.error(error.message);
      process.exit(1);
    },
  );
}
