// Покрытия локации: слои материалов на рельефе и серые маски, по которым они лежат («Свет и
// материалы», «Покрытия рельефа»). Слой описания — материал и правило, по которому строится его
// маска; правило смотрит на сетку высот и линии описания. Слой с `slope` ложится ещё и на крутые
// склоны — это решает движок, построитель пишет число в файл рельефа как есть и маски для него не
// рисует. Края масок сдвигает шум, чтобы граница не выходила циркульной.

import { isNumber, readLine } from "./curve.mjs";
import { fractal } from "./noise.mjs";
import { derivedSeed, nameHash } from "./random.mjs";
import { smoothstep, spread } from "./terrain.mjs";

/** Точек маски на клетку сцены. */
export const MASK_PER_CELL = 4;

const MAX_LAYERS = 8;

// Правила слоя: ключи сверх `material` и `rule`.
const RULES = {
  patches: { required: ["share", "wavelength"], optional: ["area", "soft"] },
  earth: { optional: ["patches", "pad", "worn"] },
  pebbles: {},
  path: { required: ["lines"] },
};

const EDGE_WAVELENGTH = 3; // клеток: волна шума на краях
const EDGE_SHIFT = 0.3; // клеток: на сколько шум сдвигает край тропы и области
const RAGGED = 0.4; // доля ступени нарастания, на которую шум сдвигает значение внутри неё
const SOFT_EDGE = 0.5; // клеток: мягкий край тропы, полосы земли и площадки
// Вытоптанная земля вдоль тропы — только если у слоя есть «worn»: столько клеток от края тропы.
const WORN_FULL = 0.2; // клеток: у самого края тропы земля вытоптана целиком
const PATCH_SOFT = 0.03; // ширина края пятна в единицах шума по умолчанию
const PATCH_FADE = 1; // клеток: пятна гаснут к краю области
const PATCH_BREAKUP = 0.12; // единиц шума: насколько мелкий шум рвёт край пятна
const BREAKUP_WAVELENGTH = 1.2; // клеток: волна мелкого шума на краю пятна
const WIDTH_WAVELENGTH = 7; // клеток: волна, по которой плавает ширина тропы
const WIDTH_VARIATION = 0.2; // доля ширины тропы, на которую она сужается и расширяется

const PEBBLES_FULL = 0.5; // выше уровня воды: галька лежит целиком
const PEBBLES_NONE = 1; // выше уровня воды: гальки нет

function maskPath(material) {
  return `terrain/${material}.png`;
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

/**
 * Проверяет слои `covers` описания. `named` — операции описания по именам: слой ссылается на них,
 * чтобы не повторять их формы. Слои возвращаются с путём маски (у первого её нет) и зерном шума.
 */
export function readCovers(covers, { seed, water, named }) {
  if (!Array.isArray(covers) || covers.length === 0 || covers.length > MAX_LAYERS) {
    throw new Error(`covers — список из 1–${MAX_LAYERS} слоёв`);
  }
  const materials = new Set();
  return covers.map((layer, index) => {
    const read = readLayer(layer, index, { seed, water, named, materials });
    materials.add(read.material);
    return read;
  });
}

function readLayer(layer, index, { seed, water, named, materials }) {
  const where = `покрытие ${index + 1}${typeof layer?.material === "string" ? ` «${layer.material}»` : ""}`;
  const fail = (message) => {
    throw new Error(`${where}: ${message}`);
  };
  if (!isObject(layer)) fail("слой — объект JSON");
  const { material, rule } = layer;
  if (typeof material !== "string" || !/^[A-Za-z0-9_-]+$/.test(material)) {
    fail("«material» — имя из латинских букв, цифр, «_» и «-»: по нему названа маска");
  }
  if (materials.has(material)) fail("этот материал уже есть в другом слое: по нему названа маска");
  if (index === 0) {
    const extra = Object.keys(layer).find((key) => key !== "material");
    if (extra) fail(`неизвестный ключ «${extra}»: первый слой лежит на всём рельефе, у него нет правила, порога крутизны и маски`);
    return { material };
  }
  const { slope } = layer;
  if (slope !== undefined && (!isNumber(slope) || slope < 0 || slope > 90)) fail("«slope» — градусы крутизны, число от 0 до 90");
  if (rule === undefined && slope === undefined) fail(`нужно «rule» (одно из: ${Object.keys(RULES).join(", ")}) или «slope»`);
  if (rule === undefined) {
    const extra = Object.keys(layer).find((key) => key !== "material" && key !== "slope");
    if (extra) fail(`неизвестный ключ «${extra}»: у слоя без «rule» маски нет, он лежит только на склонах круче «slope»`);
    return { material, slope };
  }
  if (!Object.hasOwn(RULES, rule)) fail(`«rule» — одно из: ${Object.keys(RULES).join(", ")}`);
  const spec = RULES[rule];
  const allowed = ["material", "rule", "slope", ...(spec.required ?? []), ...(spec.optional ?? [])];
  const extra = Object.keys(layer).find((key) => !allowed.includes(key));
  if (extra) fail(`неизвестный ключ «${extra}»`);
  const missing = (spec.required ?? []).find((key) => layer[key] === undefined);
  if (missing) fail(`нет обязательного ключа «${missing}»`);
  const base = { material, rule, slope, mask: maskPath(material), seed: derivedSeed(seed, nameHash(`покрытие ${material}`)) };

  switch (rule) {
    case "patches":
      return { ...base, patches: readPatches(layer, fail, { areaRequired: false, where: "" }) };
    case "earth":
      if (layer.worn !== undefined && (!isNumber(layer.worn) || layer.worn <= WORN_FULL)) fail(`«worn» — число больше ${WORN_FULL}`);
      return {
        ...base,
        patches: layer.patches === undefined ? undefined : readPatches(layer.patches, fail, { areaRequired: true, where: "«patches»: " }),
        pad: readNamed(layer.pad, "pad", named, fail),
        worn: layer.worn,
      };
    case "pebbles":
      if (water === undefined) fail("галька лежит у воды, а в описании нет «water»");
      return { ...base, level: water.level };
    case "path":
      return { ...base, lines: readLines(layer.lines, fail) };
    default:
      return base;
  }
}

// Ключ слоя называет и вид операции, на которую он ссылается: `pad` — на площадку.
function readNamed(name, key, named, fail) {
  if (name === undefined) return undefined;
  const found = typeof name === "string" ? named.get(name) : undefined;
  if (found?.op !== key) fail(`«${key}» — имя операции «${key}» из описания`);
  return found;
}

/**
 * Пятна по шуму: `share` — доля площади, `wavelength` — размер пятен в клетках, `soft` — ширина
 * края в единицах шума, `area` — область, вне которой пятен нет. У слоя «patches» ключи лежат в самом
 * слое и область не обязательна, у «earth» — в его ключе «patches», и область обязательна.
 */
function readPatches(patches, fail, { areaRequired, where }) {
  if (!isObject(patches)) fail("«patches» — объект { area, share, wavelength, soft }");
  if (where !== "") {
    const extra = Object.keys(patches).find((key) => !["area", "share", "wavelength", "soft"].includes(key));
    if (extra) fail(`${where}неизвестный ключ «${extra}»`);
  }
  if (areaRequired && patches.area === undefined) fail(`${where}нет обязательного ключа «area»`);
  const { share = 0.25, wavelength = 8, soft = PATCH_SOFT } = patches;
  if (!isNumber(share) || share <= 0 || share >= 1) fail(`${where}«share» — число больше 0 и меньше 1`);
  if (!isNumber(wavelength) || wavelength <= 0) fail(`${where}«wavelength» — число больше 0`);
  if (!isNumber(soft) || soft <= 0) fail(`${where}«soft» — число больше 0`);
  const area = patches.area === undefined ? undefined : readLine(patches.area, "patches.area", { closed: true, sharp: false }, fail);
  return { area, share, wavelength, soft };
}

function readLines(lines, fail) {
  if (!Array.isArray(lines) || lines.length === 0) fail("«lines» — непустой список троп");
  return lines.map((line, index) => {
    const at = `«lines»[${index + 1}]`;
    if (!isObject(line)) fail(`${at} — объект { points, width }`);
    const extra = Object.keys(line).find((key) => !["name", "points", "width"].includes(key));
    if (extra) fail(`${at}: неизвестный ключ «${extra}»`);
    if (line.name !== undefined && (typeof line.name !== "string" || line.name === "")) fail(`${at}: «name» — непустой текст`);
    if (!isNumber(line.width) || line.width <= 0) fail(`${at}: «width» — число больше 0`);
    return { width: line.width, path: readLine(line.points, `${at}.points`, { closed: false, sharp: false }, fail) };
  });
}

/**
 * Нарастание от 0 при `from` до 1 при `full` (`from` может быть больше `full` — убывание). Шум
 * сдвигает значение внутри ступени, а на её концах ничего не меняет: край рваный, а «целиком» и
 * «нет» остаются целиком и нет.
 */
function raggedRamp(value, from, full, noise) {
  const t = Math.min(1, Math.max(0, (value - from) / (full - from)));
  return smoothstep(0, 1, Math.min(1, Math.max(0, t + RAGGED * noise * 4 * t * (1 - t))));
}

/** Дальше от рамки `box` на `reach` клеток точка `(x, y)` края не касается и расстояние до линии считать не надо. */
function isBeyond(box, reach, x, y) {
  return x < box.minX - reach || x > box.maxX + reach || y < box.minY - reach || y > box.maxY + reach;
}

/**
 * Тропы: 1 на тропе и до `full` клеток от её края, дальше мягко до 0 в `none` клетках. Ширина тропы
 * плавает по шуму `widthNoise` на `WIDTH_VARIATION` своей доли, край ходит по шуму `noise`.
 */
function corridor(lines, full, none, x, y, noise, widthNoise) {
  let value = 0;
  for (const { width, path } of lines) {
    const reach = (width / 2) * (1 + WIDTH_VARIATION) + none + EDGE_SHIFT;
    if (isBeyond(path.box, reach, x, y)) continue;
    const half = (width / 2) * (1 + WIDTH_VARIATION * widthNoise(x / WIDTH_WAVELENGTH, y / WIDTH_WAVELENGTH));
    const d = path.nearest([x, y]).distance + EDGE_SHIFT * noise;
    value = Math.max(value, 1 - smoothstep(half + full, half + none, d));
  }
  return value;
}

/** Шум ширины троп: один на все тропы описания, чтобы вытоптанная земля шла вдоль того же края, что тропа. */
function widthNoiseOf(layers) {
  const path = layers.find((layer) => layer.rule === "path");
  const noise = fractal(derivedSeed(path?.seed ?? 0, 1), 2);
  return (x, y) => spread(noise(x, y));
}

// Каждое правило получает слой и общее (`grid`, все слои) и возвращает поле `(x, y, noise) → 0…1`.
const FIELDS = {
  patches: (layer, { grid }) => patchField(layer.patches, layer, grid),
  earth(layer, { grid, layers }) {
    const tracks = layers.find((other) => other.rule === "path")?.lines ?? [];
    const widthNoise = widthNoiseOf(layers);
    const patches = layer.patches && patchField(layer.patches, layer, grid);
    return (x, y, noise) => {
      let value = tracks.length > 0 && layer.worn ? corridor(tracks, WORN_FULL, layer.worn, x, y, noise, widthNoise) : 0;
      if (patches) value = Math.max(value, patches(x, y));
      if (layer.pad && !isBeyond(layer.pad.area.box, SOFT_EDGE + EDGE_SHIFT, x, y)) {
        value = Math.max(value, 1 - smoothstep(0, SOFT_EDGE, layer.pad.area.signedDistance([x, y]) + EDGE_SHIFT * noise));
      }
      return value;
    };
  },
  pebbles: (layer, { grid }) => (x, y, noise) =>
    raggedRamp(grid.heightAt(x, y), layer.level + PEBBLES_NONE, layer.level + PEBBLES_FULL, noise),
  path(layer, { layers }) {
    const widthNoise = widthNoiseOf(layers);
    return (x, y, noise) => corridor(layer.lines, 0, SOFT_EDGE, x, y, noise, widthNoise);
  },
};

/** Вызывает `fn(x, y, index)` для каждой точки маски: `x`, `y` — середина её квадрата в клетках сцены. */
function eachMaskPoint(grid, fn) {
  const width = grid.width * MASK_PER_CELL;
  const height = grid.height * MASK_PER_CELL;
  for (let row = 0; row < height; row++) {
    for (let col = 0; col < width; col++) fn((col + 0.5) / MASK_PER_CELL, (row + 0.5) / MASK_PER_CELL, row * width + col);
  }
}

/**
 * Пятна: шум, порог которого выбран так, чтобы пятна занимали `share` площади — всей сцены или
 * области. Край пятна шириной `soft` в единицах шума. К краю области пятна гаснут: частокол не рвёт
 * их по линии.
 */
function patchField({ area, share, wavelength, soft }, { seed, material }, grid) {
  const coarse = fractal(seed + 1);
  const fine = fractal(seed + 2, 3);
  // Мелкий шум рвёт край пятна клочками по клетке, а не ведёт его плавной дугой.
  const noise = (x, y) => coarse(x / wavelength, y / wavelength) + PATCH_BREAKUP * fine(x / BREAKUP_WAVELENGTH, y / BREAKUP_WAVELENGTH);
  const values = [];
  eachMaskPoint(grid, (x, y) => {
    if (!area || area.contains([x, y])) values.push(noise(x, y));
  });
  if (values.length === 0) throw new Error(`покрытие «${material}»: область пятен не задевает сцену`);
  values.sort((a, b) => a - b);
  const threshold = values[Math.min(values.length - 1, Math.floor((1 - share) * values.length))];
  return (x, y) => {
    if (area && !area.contains([x, y])) return 0;
    const patch = smoothstep(threshold - soft, threshold + soft, noise(x, y));
    if (!area || patch === 0) return patch;
    return patch * smoothstep(0, PATCH_FADE, -area.signedDistance([x, y]));
  };
}

/**
 * Маски слоёв, кроме первого: серые точки, по `MASK_PER_CELL` на клетку, верхний ряд — ряд сцены 0.
 * Точка маски — середина своего квадрата сцены.
 */
export function coverMasks(grid, layers) {
  const width = grid.width * MASK_PER_CELL;
  const height = grid.height * MASK_PER_CELL;
  return layers
    .filter((layer) => layer.rule)
    .map((layer) => {
      const field = FIELDS[layer.rule](layer, { grid, layers });
      const noise = fractal(layer.seed, 3);
      const pixels = new Uint8Array(width * height);
      eachMaskPoint(grid, (x, y, index) => {
        const shift = spread(noise(x / EDGE_WAVELENGTH, y / EDGE_WAVELENGTH));
        pixels[index] = Math.round(255 * Math.min(1, Math.max(0, field(x, y, shift))));
      });
      return { material: layer.material, mask: layer.mask, width, height, pixels };
    });
}
