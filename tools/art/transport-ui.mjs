// Детали интерфейса Транспортной компании для игры: набор `art/transport/ui/kit` уменьшается до размера на
// экране, а желобок и заливки шкал и кружок с числом сообщений рисуются здесь. Движок не делает уменьшенных копий
// картинки, и сильно сжатая на экране деталь рябит, поэтому в игру кладутся готовые картинки в размер.
// Размеры — в точках образца `art/transport/ui/concept-3.png`: интерфейс игры расставлен по нему
// (`ui_reference` в screens.json), масштабы деталей — из `kit.md`. Поля растягивания уменьшенной детали
// печатаются для `files.images` в game.json.
// Запуск из tools/art: node transport-ui.mjs [папка]  — по умолчанию games/transport/ui.

import { mkdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const HERE = dirname(fileURLToPath(import.meta.url));
const KIT = resolve(HERE, "../../art/transport/ui/kit");
const OUT = resolve(process.argv[2] ?? join(HERE, "../../games/transport/ui"));

// Деталь набора: файл, во сколько раз уменьшить и поля в точках исходной картинки (kit.md). Масштаб
// подобран так, чтобы поле уменьшенной детали вышло целым числом точек.
const PIECES = [
  { src: "panel", out: "panel", scale: 34 / 90, slice: [90, 90, 90, 90] },
  { src: "window", out: "window", scale: 0.53, slice: [72, 152, 75, 102] },
  { src: "inset", out: "inset", scale: 0.5, slice: [44, 44, 44, 44] },
  { src: "inset-dark", out: "inset-dark", scale: 18 / 44, slice: [44, 44, 44, 44] },
  { src: "tooltip", out: "tooltip", scale: 6 / 45, slice: [45, 45, 45, 45] },
  { src: "tooltip", out: "toast", scale: 11 / 45, slice: [45, 45, 45, 45] },
];
// Таблички: мелкие — кнопки со значком, крупные — заголовки и кнопки с надписью.
// Кремовые — включённые кнопки со значком; тёмная — кнопка с надписью, которую сейчас нельзя нажать.
for (const kind of ["blue", "blue-hover", "blue-pressed", "cream", "cream-hover", "cream-pressed"]) {
  PIECES.push({ src: `plaque-${kind}`, out: `plaque-${kind}`, scale: 9 / 88, slice: [88, 88, 88, 88] });
}
for (const kind of ["blue", "blue-hover", "blue-pressed", "blue-disabled"]) {
  PIECES.push({ src: `plaque-${kind}`, out: `plaque-wide-${kind}`, scale: 21 / 88, slice: [88, 88, 88, 88] });
}

// Значки: имя в наборе, имя в игре, сторона в точках образца (0 — силуэт, обрезанный по рисунку, см. ниже), нужен
// ли синий вид для кремовой таблички.
const ICONS = [
  ["calendar", "calendar", 34, false],
  ["coins", "coins", 34, false],
  ["pause", "pause", 32, true],
  ["play", "play", 32, true],
  ["fast", "fast", 32, true],
  ["faster", "faster", 32, true],
  ["truck", "truck", 38, true],
  ["contracts", "contracts", 36, true],
  ["firm", "firm", 36, true],
  ["mail", "mail", 36, true],
  ["settings", "settings", 34, true],
  ["truck", "truck-big", 0, true],
  ["truck-van", "truck-van-big", 0, true],
  ["truck-dump", "truck-dump-big", 0, true],
  ["scales", "scales", 36, false],
  ["fuel", "fuel", 36, false],
  ["engine", "engine", 36, false],
  ["wrench", "wrench", 36, false],
  ["eye", "eye", 38, false],
  ["route", "route", 38, false],
  ["service", "service", 38, false],
];

// Силуэты машин в заголовке карточки и в окне покупки — по ширине 56 (kit.md), без прозрачных полей вокруг:
// квадратный значок с полями вылез бы из строки списка. Все силуэты обрезаются одной рамкой — по всем сразу, —
// чтобы машины на них были в одном масштабе: короткий самосвал не растягивается до длины бортовой.
const TRUCK_BIG_W = 56;

/** Рамка непрозрачных точек значка `file`: [x0, y0, x1, y1]. */
async function opaqueBox(file) {
  const { data, info } = await sharp(file).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  let x0 = info.width, y0 = info.height, x1 = -1, y1 = -1;
  for (let y = 0; y < info.height; y++) {
    for (let x = 0; x < info.width; x++) {
      if (data[(y * info.width + x) * 4 + 3] > 8) {
        x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y);
      }
    }
  }
  return [x0, y0, x1, y1];
}

/** Значок `file`, обрезанный рамкой `box` и уменьшенный до ширины `width`. */
async function trimmedIcon(file, [x0, y0, x1, y1], width, out) {
  const height = Math.round(((y1 - y0 + 1) * width) / (x1 - x0 + 1));
  await sharp(file).extract({ left: x0, top: y0, width: x1 - x0 + 1, height: y1 - y0 + 1 })
    .resize(width, height, { kernel: "lanczos3" }).png().toFile(out);
  return [width, height];
}

// Шкала: тёмный желобок со светлым краем и скруглённая заливка внутри него, как на образце. Заливка — в
// картинке той же высоты с прозрачной каймой 2 точки: шкала рисует желобок подложкой и заливку поверх
// одним прямоугольником.
const GAUGE_W = 40;
const GAUGE_H = 13;
const SS = 8;  // рисуем в восемь раз крупнее и уменьшаем — так края гладкие
const FILLS = {
  blue: ["#79b3f2", "#3f80d3"],
  yellow: ["#f6cd73", "#d79d36"],
  cream: ["#efe6cf", "#c9bc9b"],
};

function groove() {
  const w = GAUGE_W * SS, h = GAUGE_H * SS, r = h / 2;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}">
    <defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#060a10"/><stop offset="0.45" stop-color="#0f1824"/><stop offset="1" stop-color="#142133"/>
    </linearGradient></defs>
    <rect x="${0.5 * SS}" y="${0.5 * SS}" width="${w - SS}" height="${h - SS}" rx="${r - 0.5 * SS}" fill="url(#g)"
      stroke="#7d8da3" stroke-opacity="0.75" stroke-width="${SS}"/>
  </svg>`;
}

function fill([top, bottom]) {
  const w = GAUGE_W * SS, h = GAUGE_H * SS, inset = 2.5 * SS, ih = h - 2 * inset;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}">
    <defs><linearGradient id="f" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="${top}"/><stop offset="1" stop-color="${bottom}"/>
    </linearGradient></defs>
    <rect x="${inset}" y="${inset}" width="${w - 2 * inset}" height="${ih}" rx="${ih / 2}" fill="url(#f)"/>
    <rect x="${inset + ih / 2}" y="${inset + SS}" width="${w - 2 * inset - ih}" height="${SS}" fill="#ffffff" fill-opacity="0.22"/>
  </svg>`;
}

// Кружок с числом непрочитанных сообщений на конверте: красная эмаль со светлым ободком.
const DOT = 22;
function dot() {
  const s = DOT * SS, c = s / 2;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${s}" height="${s}">
    <defs><radialGradient id="d" cx="0.4" cy="0.35" r="0.7">
      <stop offset="0" stop-color="#f05a4c"/><stop offset="1" stop-color="#b9271d"/>
    </radialGradient></defs>
    <circle cx="${c}" cy="${c}" r="${c - 1.2 * SS}" fill="url(#d)" stroke="#f3ead6" stroke-width="${1.6 * SS}"/>
  </svg>`;
}

async function svgToPng(svg, width, height, file) {
  await sharp(Buffer.from(svg)).resize(width, height, { kernel: "lanczos3" }).png().toFile(file);
}

await mkdir(join(OUT, "icons"), { recursive: true });
const images = {};
for (const p of PIECES) {
  const meta = await sharp(join(KIT, `${p.src}.png`)).metadata();
  const width = Math.round(meta.width * p.scale);
  const height = Math.round(meta.height * p.scale);
  await sharp(join(KIT, `${p.src}.png`)).resize(width, height, { kernel: "lanczos3" }).png()
    .toFile(join(OUT, `${p.out}.png`));
  images[p.out] = { path: `ui/${p.out}.png`, slice: p.slice.map((v) => Math.round(v * p.scale)), smooth: true };
}
const silhouettes = await Promise.all(
  ICONS.filter(([, , size]) => size === 0).map(([src]) => opaqueBox(join(KIT, "icons", `${src}.png`))));
const silhouetteBox = [
  Math.min(...silhouettes.map((b) => b[0])), Math.min(...silhouettes.map((b) => b[1])),
  Math.max(...silhouettes.map((b) => b[2])), Math.max(...silhouettes.map((b) => b[3])),
];
for (const [src, out, size, dark] of ICONS) {
  for (const suffix of dark ? ["", "-dark"] : [""]) {
    const file = join(OUT, "icons", `${out}${suffix}.png`);
    if (size === 0) {
      const icon = join(KIT, "icons", `${src}${suffix}.png`);
      console.error(`${out}: ${(await trimmedIcon(icon, silhouetteBox, TRUCK_BIG_W, file)).join(" × ")}`);
    } else {
      await sharp(join(KIT, "icons", `${src}${suffix}.png`)).resize(size, size, { kernel: "lanczos3" }).png().toFile(file);
    }
    images[`icon_${out}${suffix}`.replaceAll("-", "_")] = { path: `ui/icons/${out}${suffix}.png`, smooth: true };
  }
}
await svgToPng(groove(), GAUGE_W, GAUGE_H, join(OUT, "gauge.png"));
images.gauge = { path: "ui/gauge.png", slice: [6, 6, 6, 6], smooth: true };
for (const [name, colors] of Object.entries(FILLS)) {
  await svgToPng(fill(colors), GAUGE_W, GAUGE_H, join(OUT, `gauge-${name}.png`));
  images[`gauge_${name}`] = { path: `ui/gauge-${name}.png`, slice: [6, 6, 6, 6], smooth: true };
}
await svgToPng(dot(), DOT, DOT, join(OUT, "dot.png"));
images.dot = { path: "ui/dot.png", smooth: true };

const named = Object.fromEntries(Object.entries(images).map(([k, v]) => [k.replaceAll("-", "_"), v]));
console.log(JSON.stringify(named, null, 2));
