// Карта цвета рельефа («Свет и материалы» → «Карта цвета»): крупный цвет поверх покрытий, как
// terrain tint в Far Cry 5 и color map в REDengine. Считается по готовой сетке высот и картам, что
// накопили операции гор: щели и ложбины темнее и не видят неба, выпуклые рёбра светлее, где стекала
// вода — тёмные потёки, камень лежит цветными пластами, а весь массив то теплее, то холоднее.
// Канал `rgb` — во сколько раз цвет земли светлее: 128 — как есть; `a` — сколько неба видно.

import { isNumber } from "./curve.mjs";
import { fractal } from "./noise.mjs";
import { derivedSeed, nameHash } from "./random.mjs";
import { rangeReach, smoothstep, spread } from "./terrain.mjs";

/** Точек карты цвета на клетку сцены — как у масок покрытий. */
export const TINT_PER_CELL = 4;
export const TINT_PATH = "terrain/tint.png";

const KEYS = ["range", "occlusion", "cavity", "edges", "streaks", "bands", "band", "warmth"];
const ZONE_FADE = 3; // клеток за подножием, на которых цвет гор набирает силу
const SKY_RADIUS = 6; // клеток: как далеко точка ищет, что заслоняет ей небо
const SKY_DIRECTIONS = 12;
const SKY_STEP = 0.35; // клеток между пробами высоты по направлению
const NEAR = 0.5; // клеток: мелкая мера выпуклости — рёбра уступов
const FAR = 2; // клеток: крупная мера — кулуары и отроги
const WARM = [1.06, 1.0, 0.9];
const COOL = [0.92, 0.97, 1.06];
const STREAK = [0.92, 0.88, 0.82]; // потёки буроватые, не просто темнее

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

/**
 * Проверяет раздел `tint` описания. `range` — имя гор, где цвет гор действует; затенение неба
 * `occlusion` — по всему рельефу. Остальные ключи — сила, от 0 до 1; `band` — толщина цветного
 * пласта в клетках.
 */
export function readTint(tint, { seed, named }) {
  const fail = (message) => {
    throw new Error(`tint: ${message}`);
  };
  if (!isObject(tint)) fail("раздел — объект JSON");
  const extra = Object.keys(tint).find((key) => !KEYS.includes(key));
  if (extra) fail(`неизвестный ключ «${extra}», есть: ${KEYS.join(", ")}`);
  const range = named.get(tint.range);
  if (range?.op !== "range") fail("«range» — имя операции «range»: горы, которые красит карта цвета");
  const share = (key, fallback) => {
    const value = tint[key] ?? fallback;
    if (!isNumber(value) || value < 0 || value > 1) fail(`«${key}» — число от 0 до 1`);
    return value;
  };
  const band = tint.band ?? 1.2;
  if (!isNumber(band) || band <= 0) fail("«band» — толщина пласта в клетках, больше 0");
  return {
    range,
    occlusion: share("occlusion", 0.8),
    cavity: share("cavity", 0.35),
    edges: share("edges", 0.2),
    streaks: share("streaks", 0.25),
    bands: share("bands", 0.12),
    warmth: share("warmth", 0.5),
    band,
    seed: derivedSeed(seed, nameHash("карта цвета")),
  };
}

/** Доля неба, которую видит точка `(x, y)`: по `SKY_DIRECTIONS` направлениям — самый высокий заслон. */
function skyView(grid, x, y) {
  const h = grid.heightAt(x, y);
  let open = 0;
  for (let k = 0; k < SKY_DIRECTIONS; k++) {
    const angle = (k / SKY_DIRECTIONS) * Math.PI * 2;
    const [dx, dy] = [Math.cos(angle), Math.sin(angle)];
    let steepest = 0;
    for (let s = SKY_STEP; s <= SKY_RADIUS; s += SKY_STEP) {
      const px = x + dx * s;
      const py = y + dy * s;
      if (px < 0 || py < 0 || px > grid.width || py > grid.height) break;
      steepest = Math.max(steepest, (grid.heightAt(px, py) - h) / s);
    }
    // Небо над направлением — от заслона до зенита: 1 − синус угла заслона.
    open += 1 - steepest / Math.hypot(1, steepest);
  }
  return open / SKY_DIRECTIONS;
}

/** Насколько точка ниже соседей на расстоянии `r` (ложбина, щель — больше нуля; ребро — меньше). */
function hollow(grid, x, y, r) {
  const around =
    (grid.heightAt(x - r, y) + grid.heightAt(x + r, y) + grid.heightAt(x, y - r) + grid.heightAt(x, y + r)) / 4;
  return (around - grid.heightAt(x, y)) / r;
}

/** Значение карты сетки `name` в месте `(x, y)` — по ближайшей точке сетки. */
function mapAt(grid, name, x, y) {
  const values = grid.maps?.get(name);
  if (!values) return 0;
  const col = Math.min(grid.cols - 1, Math.max(0, Math.round(x * grid.density)));
  const row = Math.min(grid.rows - 1, Math.max(0, Math.round(y * grid.density)));
  return values[row * grid.cols + col];
}

/** Карта цвета: `width × height` точек по четыре байта, верхний ряд — ряд сцены 0. */
export function tintMap(grid, tint) {
  const width = grid.width * TINT_PER_CELL;
  const height = grid.height * TINT_PER_CELL;
  const reach = rangeReach(tint.range);
  const warm = fractal(tint.seed, 3);
  const bend = fractal(tint.seed + 1, 3);
  let flowTop = 0;
  for (const value of grid.maps?.get("flow") ?? []) flowTop = Math.max(flowTop, value);
  const flowScale = flowTop > 0 ? 1 / Math.log1p(flowTop) : 0;
  const pixels = new Uint8Array(width * height * 4);
  for (let row = 0; row < height; row++) {
    for (let col = 0; col < width; col++) {
      const x = (col + 0.5) / TINT_PER_CELL;
      const y = (row + 0.5) / TINT_PER_CELL;
      const index = (row * width + col) * 4;
      const sky = skyView(grid, x, y);
      pixels[index + 3] = Math.round(255 * (1 - tint.occlusion * (1 - sky)));
      const zone = smoothstep(0, ZONE_FADE, reach(x, y));
      let color = [1, 1, 1];
      if (zone > 0) {
        const near = hollow(grid, x, y, NEAR);
        const far = hollow(grid, x, y, FAR);
        const crevice = smoothstep(0, 0.6, near) * 0.6 + smoothstep(0, 0.4, far) * 0.4;
        const edge = smoothstep(0, 0.6, -near);
        const flow = Math.log1p(mapAt(grid, "flow", x, y)) * flowScale;
        const h = grid.heightAt(x, y);
        const layer = Math.floor((h + 0.6 * spread(bend(x / 9, y / 9))) / tint.band);
        const shade = derivedSeed(tint.seed, layer) / 4294967296;
        const t = smoothstep(-0.5, 0.5, spread(warm(x / 20, y / 20)) * tint.warmth);
        color = color.map((_, c) => {
          let m = COOL[c] + (WARM[c] - COOL[c]) * t;
          m *= 1 + tint.bands * (shade * 2 - 1);
          m *= 1 - tint.cavity * crevice;
          m *= 1 + tint.edges * edge;
          m *= 1 - tint.streaks * smoothstep(0.35, 0.9, flow) * (1 - STREAK[c] * 0.5);
          return 1 + (m - 1) * zone;
        });
      }
      for (let c = 0; c < 3; c++) pixels[index + c] = Math.round(Math.min(255, Math.max(0, 128 * color[c])));
    }
  }
  return { width, height, pixels };
}
