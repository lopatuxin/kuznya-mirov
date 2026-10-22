// Значки интерфейса Транспортной компании: кнопки полосы и строки окон. Знак плоский, одного цвета, на
// прозрачном фоне; ложится на эмалевую табличку кнопки или на площадку окна. Каждый значок — в двух цветах:
// светлый для синих табличек и площадок, синий (`-dark`) для кремовой таблички включённой кнопки.
// Полуторка — по виду сбоку модели `art/transport/models/gaz_aa.py`: кузов, кабина с козырьком, капот, крыло.
// Рисуются из SVG в PNG 128 × 128; лист для просмотра — `sheet.png` в той же папке.
// Запуск из tools/art: node ui-icons.mjs [папка]  — по умолчанию art/transport/ui/kit/icons.

import { mkdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const HERE = dirname(fileURLToPath(import.meta.url));
const LIGHT = "#efe9dc";
const DARK = "#123c8c";
const PLAQUE = resolve(HERE, "../../art/transport/ui/kit/plaque-blue.png");

// Знак в квадрате 100 × 100. Слой — `ink` (что залить цветом знака) и `cut` (что вырезать из этого слоя);
// вырез одного слоя не задевает другие, так колесо отделяется от крыла зазором, а само остаётся целым.
const F = `fill="currentColor"`;
const L = (w) => `fill="none" stroke="currentColor" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"`;
const CF = `fill="#000"`;
const CL = (w) => `fill="none" stroke="#000" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"`;
const rect = (x0, y0, x1, y1, rx = 0, a = F) => `<rect x="${x0}" y="${y0}" width="${x1 - x0}" height="${y1 - y0}" rx="${rx}" ${a}/>`;
const circle = (cx, cy, r, a = F) => `<circle cx="${cx}" cy="${cy}" r="${r}" ${a}/>`;

/** Контур шестерни: `teeth` зубцов между радиусами `inner` и `outer`. */
function gear(cx, cy, outer, inner, teeth) {
  const pts = [];
  for (let k = 0; k < teeth * 4; k++) {
    const a = (k / (teeth * 4)) * Math.PI * 2 + Math.PI / (teeth * 4);
    const r = k % 4 < 2 ? outer : inner;
    pts.push(`${(cx + r * Math.cos(a)).toFixed(1)},${(cy + r * Math.sin(a)).toFixed(1)}`);
  }
  return `M${pts.join("L")}z`;
}

/** Ребро монеты в стопке: полоса высотой `h` с выгнутыми вниз верхом и низом. */
const coin = (cx, y, rx, h) => `<path ${F} d="M${cx - rx} ${y}A${rx} 4 0 0 0 ${cx + rx} ${y}V${y + h}A${rx} 4 0 0 1 ${cx - rx} ${y + h}Z"/>`;
/** Лицевая сторона верхней монеты. */
const face = (cx, cy, rx) => `<ellipse cx="${cx}" cy="${cy}" rx="${rx}" ry="5" ${F}/>`;
/** Ободок на лицевой стороне монеты — вырез. */
const faceRing = (cx, cy, rx) => `<ellipse cx="${cx}" cy="${cy}" rx="${rx - 5}" ry="2.2" ${CL(1.6)}/>`;
/** Контур стопки монет с запасом `pad` вокруг — вырез, отделяющий переднюю стопку от задней. */
const coinStack = (cx, top, rx, pad) => `M${cx - rx - pad} ${top}A${rx + pad} ${5 + pad} 0 0 1 ${cx + rx + pad} ${top}V${88 + pad}H${cx - rx - pad}Z`;

const WRENCH ="M64 14a20 20 0 0 0-18 27L16 71a8 8 0 0 0 11 11l30-30a20 20 0 0 0 27-18l-12 6-10-6-1-12z";

const ICONS = {
  // Дата — листок календаря с кольцами и клетками дней.
  calendar: [{
    ink: rect(14, 22, 86, 86, 8) + rect(29, 12, 37, 30, 3) + rect(63, 12, 71, 30, 3),
    cut: rect(21, 38, 79, 79, 3, CF),
  }, {
    ink: [27, 40, 53, 66].flatMap((x) => [45, 57, 69].map((y) => rect(x, y, x + 8, y + 8, 1.5))).join(""),
  }],
  // Скорость: пауза, ход, быстрее, ещё быстрее.
  pause: [{ ink: rect(26, 20, 43, 80, 4) + rect(57, 20, 74, 80, 4) }],
  play: [{ ink: `<path ${F} stroke="currentColor" stroke-width="6" stroke-linejoin="round" d="M33 20L79 50L33 80Z"/>` }],
  fast: [{ ink: `<path ${F} stroke="currentColor" stroke-width="5" stroke-linejoin="round" d="M14 24L50 50L14 76ZM50 24L86 50L50 76Z"/>` }],
  faster: [{ ink: `<path ${F} stroke="currentColor" stroke-width="4" stroke-linejoin="round" d="M7 27L34 50L7 73ZM36 27L63 50L36 73ZM65 27L92 50L65 73Z"/>` }],
  // Деньги — две стопки монет: ребро каждой монеты выгнуто, у верхней видна лицевая сторона.
  coins: [{
    ink: [70, 61, 52, 43].map((y) => coin(34, y, 22, 7)).join("") + face(34, 40, 22),
    cut: `<path ${CF} d="${coinStack(68, 57, 20, 3)}"/>` + faceRing(34, 40, 22),
  }, {
    ink: [78, 69, 60].map((y) => coin(68, y, 20, 7)).join("") + face(68, 57, 20),
    cut: faceRing(68, 57, 20),
  }],
  // Стройка дорог — дорога вдаль: края, разметка посередине и плюс.
  road: [{
    ink: `<path ${L(9)} d="M10 90L37 24M90 90L63 24"/>` + `<path ${F} d="M48 26H52L52.6 36H47.4ZM46.6 44H53.4L54.4 58H45.6ZM45 66H55L56.4 88H43.6Z"/>`,
  }, {
    ink: rect(81, 6, 87, 30, 2) + rect(72, 15, 96, 21, 2),
  }],
  // Стройка предприятий — цех с пилообразной крышей и труба.
  factory: [{
    ink: `<path ${F} d="M8 88V44L28 32V44L48 32V44L68 32V88Z"/>` + rect(72, 14, 86, 88, 2) + rect(70, 10, 88, 16, 2),
    cut: rect(16, 60, 26, 72, 1, CF) + rect(34, 60, 44, 72, 1, CF) + rect(52, 60, 62, 72, 1, CF),
  }],
  // Машины — полуторка сбоку, перед вправо. Мерки сняты с вида модели сбоку: длина 92, высота втрое меньше
  // и вытянута в 1,2 раза, чтобы читалась в кнопке; колёса чуть крупнее настоящих.
  truck: [{
    ink: rect(4, 40, 49.5, 53, 1) + rect(4, 53, 92, 58.5) + rect(36, 57.5, 74, 60.5) +
      rect(50, 30, 68.5, 58, 1) + rect(49, 27.5, 70.5, 31, 1.5) +
      `<path ${F} d="M68 40L88 40.5V55H68Z"/>` + rect(87, 38.5, 91.5, 56, 1.2) +
      `<path ${F} d="M72.4 63.5A13 13 0 0 1 98.4 63.5H95.9A10.5 10.5 0 0 0 74.9 63.5Z"/>`,
    cut: rect(56.5, 32.5, 66, 40, 1, CF) + rect(7, 45.5, 47, 47.5, 0, CF) + circle(25.7, 63.5, 10.5, CF) + circle(85.4, 63.5, 10.5, CF),
  }, {
    ink: circle(25.7, 63.5, 9) + circle(85.4, 63.5, 9),
    cut: circle(25.7, 63.5, 3, CF) + circle(85.4, 63.5, 3, CF),
  }],
  // Договоры — лист с загнутым углом, строками и круглой печатью.
  contracts: [{
    ink: `<path ${F} d="M20 8H60L78 26V92H20Z"/>`,
    cut: `<path ${CL(4)} d="M60 8V26H78"/>` + rect(28, 32, 66, 37, 2, CF) + rect(28, 43, 66, 48, 2, CF) + rect(28, 54, 52, 59, 2, CF) + circle(64, 76, 18, CF),
  }, {
    ink: circle(64, 76, 14),
    cut: circle(64, 76, 9, CL(3)),
  }],
  // Фирма — контора: дом в несколько этажей с крылом.
  firm: [{
    ink: rect(18, 12, 60, 90, 2) + rect(60, 38, 84, 90, 2),
    cut: [22, 34, 46, 58].flatMap((y) => [rect(25, y, 34, y + 8, 1, CF), rect(44, y, 53, y + 8, 1, CF)]).join("") +
      [46, 58, 70].map((y) => rect(67, y, 77, y + 8, 1, CF)).join("") + rect(33, 74, 45, 90, 1, CF),
  }],
  // Сообщения — конверт.
  mail: [{ ink: rect(10, 24, 90, 76, 6), cut: `<path ${CL(5)} d="M15 30L50 55L85 30"/>` }],
  // Настройки — шестерня.
  settings: [{ ink: `<path ${F} d="${gear(50, 50, 42, 32, 8)}"/>`, cut: circle(50, 50, 13, CF) }],
  // Груз — весы с двумя чашами.
  scales: [{
    ink: rect(47, 18, 53, 82, 2) + rect(28, 82, 72, 90, 3) + rect(12, 24, 88, 30, 3) + circle(50, 17, 6) +
      `<path ${L(3)} d="M20 30L9 56M20 30L31 56M80 30L69 56M80 30L91 56"/>` +
      `<path ${F} d="M5 56H35A15 12 0 0 1 5 56ZM65 56H95A15 12 0 0 1 65 56Z"/>`,
  }],
  // Бензин — колонка, как на значке заправки.
  fuel: [{
    ink: rect(20, 14, 56, 84, 3) + rect(14, 80, 62, 89, 2) + `<path ${L(6)} d="M56 34h10c6 0 8 4 8 9v27c0 4 3 6 6 6s6-2 6-6V44l-8-8"/>`,
    cut: rect(27, 22, 49, 40, 2, CF),
  }],
  // Износ — двигатель, как лампа неисправности мотора на приборной доске.
  engine: [{ ink: `<path ${L(6)} d="M22 38H36V30H58V38H68L74 44H82V38H90V68H82V62H74L66 72H38L30 64H22V58H12V44H22Z"/>` }],
  // Срок ТО — гаечный ключ.
  wrench: [{ ink: `<path ${F} d="${WRENCH}"/>` }],
  // Следить камерой — глаз.
  eye: [{
    ink: `<path ${F} d="M6 50C22 24 78 24 94 50C78 76 22 76 6 50Z"/>`,
    cut: circle(50, 50, 17, CF),
  }, { ink: circle(50, 50, 9) }],
  // План — путь между двумя точками.
  route: [{ ink: circle(24, 28, 9, L(6)) + circle(76, 72, 9, L(6)) + `<path ${L(6)} d="M30 36C40 58 56 38 68 64"/>` }],
  // Отправить на ТО — ключ и стрелка.
  service: [{
    ink: `<g transform="translate(-2 4) scale(0.72)"><path ${F} d="${WRENCH}"/></g>` + `<path ${L(7)} d="M58 74H90M80 64L90 74L80 84"/>`,
  }],
};

const svg = (layers, color) => {
  const defs = layers.map((l, i) => (l.cut ? `<mask id="c${i}"><rect width="100" height="100" fill="#fff"/>${l.cut}</mask>` : "")).join("");
  const body = layers.map((l, i) => `<g${l.cut ? ` mask="url(#c${i})"` : ""}>${l.ink}</g>`).join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 100 100" color="${color}"><defs>${defs}</defs>${body}</svg>`;
};

const out = resolve(process.argv[2] ?? resolve(HERE, "../../art/transport/ui/kit/icons"));
await mkdir(out, { recursive: true });
const tiles = [];
for (const [id, layers] of Object.entries(ICONS)) {
  const light = await sharp(Buffer.from(svg(layers, LIGHT))).png().toBuffer();
  await sharp(light).toFile(join(out, `${id}.png`));
  await sharp(Buffer.from(svg(layers, DARK))).png().toFile(join(out, `${id}-dark.png`));
  tiles.push({ id, light });
}
// Лист для просмотра: каждый значок на синей табличке в размере кнопки полосы (52) и вдвое крупнее.
const plaque = (size) => sharp(PLAQUE).resize(size, size, { fit: "fill" }).png().toBuffer();
const small = await plaque(52), big = await plaque(104);
const cols = 6, cw = 190, ch = 150;
const layers = [];
for (const [n, { id, light }] of tiles.entries()) {
  const x = (n % cols) * cw + 16, y = Math.floor(n / cols) * ch + 16;
  layers.push({ input: small, left: x, top: y + 26 });
  layers.push({ input: await sharp(light).resize(32, 32).toBuffer(), left: x + 10, top: y + 36 });
  layers.push({ input: big, left: x + 66, top: y });
  layers.push({ input: await sharp(light).resize(64, 64).toBuffer(), left: x + 86, top: y + 20 });
  layers.push({ input: Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="${cw}" height="24"><text x="4" y="17" font-family="Arial" font-size="14" fill="#c9d3dd">${id}</text></svg>`), left: x - 4, top: y + 110 });
}
const rows = Math.ceil(tiles.length / cols);
await sharp({ create: { width: cols * cw + 16, height: rows * ch + 16, channels: 4, background: "#24344a" } }).composite(layers).png().toFile(join(out, "sheet.png"));
console.log(`значки — в ${out}`);
