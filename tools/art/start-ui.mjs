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

// Экран настроек на светлой дымке паузы: полоса ползунка — тонкая серая черта, заливка — красная черта толще,
// бегунок — красный узел, как на черте под «Паузой», на пятне пергамента, чтобы полоса не просвечивала сквозь узел.
// Переключатель — красный ромб-рамка, включённый — с узлом внутри. Полоса и заливка тянутся полями `slice` по 12.
const PARCHMENT = "#efe2cc";
const INK_GRAY = "#7d6b5c";

function sliderLine(width, height, thickness, color) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">
    <rect x="0" y="${(height - thickness) / 2}" width="${width}" height="${thickness}" rx="${thickness / 2}" fill="${color}"/>
  </svg>`;
}

function sliderKnob(size) {
  const c = size / 2;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}">
    <defs>
      <radialGradient id="p" cx="50%" cy="50%" r="50%">
        <stop offset="0.6" stop-color="${PARCHMENT}" stop-opacity="1"/>
        <stop offset="1" stop-color="${PARCHMENT}" stop-opacity="0"/>
      </radialGradient>
    </defs>
    <circle cx="${c}" cy="${c}" r="${c}" fill="url(#p)"/>
    ${ornament(c, c, size * 0.15, INK_RED, PARCHMENT)}
  </svg>`;
}

function switchBox(size, on) {
  const c = size / 2;
  const outer = size * 0.46;
  const inner = outer - size * 0.07;
  const diamond = (s) => `${c},${c - s} ${c + s},${c} ${c},${c + s} ${c - s},${c}`;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}">
    <path d="M${diamond(outer)}Z M${diamond(inner)}Z" fill="${INK_RED}" fill-rule="evenodd"/>
    ${on ? ornament(c, c, size * 0.1, INK_RED, PARCHMENT) : ""}
  </svg>`;
}

const folder = process.argv[2];
if (!folder) {
  console.error("node tools/art/start-ui.mjs <папка>");
  process.exit(1);
}
await sharp(Buffer.from(menuGlow(880, 168))).png().toFile(join(folder, "menu_glow.png"));
await sharp(Buffer.from(parchmentGlow(880, 168))).png().toFile(join(folder, "parchment_glow.png"));
console.log("menu_glow.png и parchment_glow.png 880×168");
await sharp(Buffer.from(sliderLine(1040, 112, 6, INK_GRAY))).png().toFile(join(folder, "settings_track.png"));
await sharp(Buffer.from(sliderLine(1040, 112, 12, INK_RED))).png().toFile(join(folder, "settings_fill.png"));
await sharp(Buffer.from(sliderKnob(112))).png().toFile(join(folder, "settings_knob.png"));
await sharp(Buffer.from(switchBox(112, false))).png().toFile(join(folder, "settings_switch_off.png"));
await sharp(Buffer.from(switchBox(112, true))).png().toFile(join(folder, "settings_switch_on.png"));
console.log("settings_track.png и settings_fill.png 1040×112, settings_knob.png и settings_switch_*.png 112×112");
