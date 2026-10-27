// Детали начального экрана «Распутья», которые рисуются программой: золотое свечение за пунктом меню под курсором
// с узорами по бокам, как у «Новой игры» на образце `art/platformer/start/sample.png`, и пустой обычный вид пункта.
// Для кнопок на светлом листе экрана героя — тот же узор красными чернилами листа с полыми ромбиками: золото на
// пергаменте сливается с фоном (`parchment_glow.png`).
// Свечение тянется на прямоугольник кнопки, поэтому его пропорции — пропорции кнопки, а размер — вдвое больше, чем
// на экране при `ui_reference` 1920 × 1080. Строки камня высекает и зажигает `carve.mjs`.
// Запуск: node tools/art/start-ui.mjs <папка>

import { join } from "node:path";
import sharp from "sharp";

const GOLD = "#ffcf6e";
const INK_RED = "#8c2416";

// Узор как у «Новой игры» на образце: четыре полых золотых ромбика крестом вокруг середины и точки на концах креста.
// `r` — половина ширины одного ромбика.
// `hole` — цвет серединки ромбика; без него серединка пустая и сквозь неё виден фон.
function ornament(cx, cy, r, color = GOLD, hole = "#3a2410") {
  const diamond = (x, y, s) => `${x},${y - s} ${x + s},${y} ${x},${y + s} ${x - s},${y}`;
  const step = r * 1.4;
  const petals = [[0, -step], [step, 0], [0, step], [-step, 0]]
    .map(([dx, dy]) => hole
      ? `<polygon points="${diamond(cx + dx, cy + dy, r)}" fill="${color}"/>
      <polygon points="${diamond(cx + dx, cy + dy, r * 0.48)}" fill="${hole}"/>`
      : `<path d="M${diamond(cx + dx, cy + dy, r)}Z M${diamond(cx + dx, cy + dy, r * 0.48)}Z" fill="${color}" fill-rule="evenodd"/>`)
    .join("");
  const tip = step + r * 1.5;
  const dots = [[0, -tip], [tip, 0], [0, tip], [-tip, 0]]
    .map(([dx, dy]) => `<polygon points="${diamond(cx + dx, cy + dy, r * 0.34)}" fill="${color}"/>`)
    .join("");
  return petals + dots;
}

function menuGlow(width, height) {
  const r = height * 0.07;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">
    <defs>
      <radialGradient id="g" cx="50%" cy="50%" r="50%">
        <stop offset="0" stop-color="${GOLD}" stop-opacity="0.5"/>
        <stop offset="0.55" stop-color="#e09a3a" stop-opacity="0.22"/>
        <stop offset="1" stop-color="#e09a3a" stop-opacity="0"/>
      </radialGradient>
    </defs>
    <ellipse cx="${width / 2}" cy="${height / 2}" rx="${width * 0.42}" ry="${height * 0.46}" fill="url(#g)"/>
    ${ornament(height * 0.62, height / 2, r)}
    ${ornament(width - height * 0.62, height / 2, r)}
  </svg>`;
}

// Свечение листа: тёплое золото гуще, чем на тёмной полосе меню, иначе на пергаменте его не видно.
function parchmentGlow(width, height) {
  const r = height * 0.07;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">
    <defs>
      <radialGradient id="g" cx="50%" cy="50%" r="50%">
        <stop offset="0" stop-color="#e8a53c" stop-opacity="0.55"/>
        <stop offset="0.55" stop-color="#d98a2b" stop-opacity="0.25"/>
        <stop offset="1" stop-color="#d98a2b" stop-opacity="0"/>
      </radialGradient>
    </defs>
    <ellipse cx="${width / 2}" cy="${height / 2}" rx="${width * 0.42}" ry="${height * 0.46}" fill="url(#g)"/>
    ${ornament(height * 0.62, height / 2, r, INK_RED, null)}
    ${ornament(width - height * 0.62, height / 2, r, INK_RED, null)}
  </svg>`;
}

const folder = process.argv[2];
if (!folder) {
  console.error("node tools/art/start-ui.mjs <папка>");
  process.exit(1);
}
await sharp(Buffer.from(menuGlow(880, 168))).png().toFile(join(folder, "menu_glow.png"));
await sharp(Buffer.from(parchmentGlow(880, 168))).png().toFile(join(folder, "parchment_glow.png"));
// Пустая картинка — обычный вид пункта меню: у кнопки картинки во всех трёх видах, свечение — только под курсором.
await sharp({ create: { width: 4, height: 4, channels: 4, background: { r: 0, g: 0, b: 0, alpha: 0 } } }).png().toFile(join(folder, "menu_clear.png"));
console.log("menu_glow.png и parchment_glow.png 880×168, menu_clear.png 4×4");
