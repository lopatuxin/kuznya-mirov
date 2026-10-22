// Значки мест Транспортной компании: над каждым местом — значок его дела вместо подписи с улицей. Знак светлый на
// круглом эмалевом значке набора интерфейса (`art/transport/ui/kit/badge-blue.png`). Род места (`ring`: своё,
// клиенты, магазины, заправка и мастерская) покажет цвет эмали; пока цвета не выбраны, все значки синие, как на
// образце экрана. Рисуются в PNG 72 × 72 — вдвое крупнее, чем над картой, чтобы не рябили при уменьшении; лист
// для просмотра — `sheet.png` в той же папке.
// Запуск из tools/art: node icons.mjs [папка]  — по умолчанию art/transport/icons.

import { mkdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const HERE = dirname(fileURLToPath(import.meta.url));
// Знак рисуется маской: белое — знак, чёрное — вырез в нём; цвет знака — `GLYPH`, как у значков кнопок.
const INK = "#ffffff";
const PLATE = "#000000";
const GLYPH = [0xef, 0xe9, 0xdc];
const BADGE = resolve(HERE, "../../art/transport/ui/kit/badge-blue.png");
const SIZE = 72;
const GLYPH_SIZE = 40;  // знак 100 × 100 ложится в середину значка между заклёпками

// Знаки в квадрате 100 × 100 с серединой в (50, 50): заливка — цвет знака, линии — `stroke` того же цвета.
const line = (w = 6) => `fill="none" stroke="${INK}" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"`;
const fill = `fill="${INK}"`;
const ICONS = {
  // Своя контора — грузовичок.
  office: { ring: "own", name: "Контора", svg: `<path ${fill} d="M18 34h38v30H18z"/><path ${fill} d="M58 42h14l10 12v10H58z"/><path fill="${PLATE}" d="M62 46h9l6 8H62z"/><circle ${fill} cx="30" cy="68" r="8"/><circle ${fill} cx="70" cy="68" r="8"/><circle fill="${PLATE}" cx="30" cy="68" r="3"/><circle fill="${PLATE}" cx="70" cy="68" r="3"/>` },
  // Хлебозавод — каравай с надрезами.
  bakery: { ring: "client", name: "Хлебозавод", svg: `<path ${fill} d="M14 62c0-18 16-30 36-30s36 12 36 30c0 5-4 8-9 8H23c-5 0-9-3-9-8z"/><path fill="none" stroke="${PLATE}" stroke-width="5" stroke-linecap="round" d="M30 50l8-9M46 50l8-10M62 50l8-9"/>` },
  // Рынок — навес с фестонами над прилавком.
  market: { ring: "client", name: "Рынок", svg: `<path ${fill} d="M16 30h68l-6 14H22z"/><path ${fill} d="M22 44c0 5 4 8 8 8s8-3 8-8c0 5 4 8 8 8s8-3 8-8c0 5 4 8 8 8s8-3 8-8c0 5 4 8 8 8z"/><path ${line(6)} d="M24 54v24M76 54v24M20 78h60"/><path ${fill} d="M30 64h40v8H30z"/>` },
  // Пристань — якорь.
  pier: { ring: "client", name: "Пристань", svg: `<circle ${line(6)} cx="50" cy="22" r="7"/><path ${line(7)} d="M50 29v52M34 40h32"/><path ${line(7)} d="M20 58c2 14 14 23 30 23s28-9 30-23"/><path ${fill} d="M14 60l10-8 4 12zM86 60l-10-8-4 12z"/>` },
  // Товарный двор станции — паровоз.
  station: { ring: "client", name: "Товарный двор", svg: `<path ${fill} d="M14 44h44v20H14z"/><path ${fill} d="M58 30h22v34H58z"/><path fill="${PLATE}" d="M63 35h12v10H63z"/><path ${fill} d="M20 30h9v14h-9z"/><path ${fill} d="M10 60l6-6v10z"/><circle ${fill} cx="26" cy="70" r="8"/><circle ${fill} cx="46" cy="70" r="8"/><circle ${fill} cx="70" cy="70" r="8"/><circle fill="${PLATE}" cx="26" cy="70" r="3"/><circle fill="${PLATE}" cx="46" cy="70" r="3"/><circle fill="${PLATE}" cx="70" cy="70" r="3"/>` },
  // Оптовая база — ящики штабелем.
  wholesale: { ring: "client", name: "Оптовая база", svg: `<path ${line(5)} d="M18 52h30v28H18zM52 52h30v28H52zM35 22h30v28H35z"/><path ${line(4)} d="M18 52l30 28M48 52L18 80M52 52l30 28M82 52L52 80M35 22l30 28M65 22L35 50"/>` },
  // Механический завод — шестерня.
  plant: { ring: "client", name: "Механический завод", svg: `<path ${fill} d="${gear(50, 50, 32, 24, 8)}"/><circle fill="${PLATE}" cx="50" cy="50" r="11"/>` },
  // Швейная фабрика — катушка ниток и игла.
  sewing: { ring: "client", name: "Швейная фабрика", svg: `<path ${fill} d="M22 22h40v8H22zM22 70h40v8H22z"/><path ${fill} d="M28 30h28v40H28z"/><path fill="none" stroke="${PLATE}" stroke-width="3" d="M28 38h28M28 46h28M28 54h28M28 62h28"/><path ${line(4)} d="M84 16L66 84"/><path fill="none" stroke="${INK}" stroke-width="3" d="M58 52c10-4 22 2 26-8"/>` },
  // Ювелирная артель — кольцо с камнем.
  jewelry: { ring: "client", name: "Ювелирная артель", svg: `<circle ${line(8)} cx="50" cy="60" r="20"/><path ${fill} d="M36 26h28l8 10-22 16-22-16z"/><path fill="none" stroke="${PLATE}" stroke-width="3" d="M28 36h44M42 26l-4 10 12 16 12-16-4-10"/>` },
  // Мебельная артель — стул.
  furniture: { ring: "client", name: "Мебельная артель", svg: `<path ${line(7)} d="M32 16v68M32 50h36v34M32 66h36"/><path ${fill} d="M28 46h44v8H28z"/><path ${line(5)} d="M32 22h14M32 34h14"/>` },
  // Стройматериалы — кладка кирпича.
  materials: { ring: "client", name: "Стройматериалы", svg: `<path ${fill} d="M14 26h34v14H14zM52 26h34v14H52zM14 44h16v14H14zM34 44h32v14H34zM70 44h16v14H70zM14 62h34v14H14zM52 62h34v14H52z"/>` },
  // Стройка — башенный кран.
  site: { ring: "client", name: "Стройка", svg: `<path ${line(6)} d="M34 84V18M14 26h70M34 18L84 26M34 18L14 26"/><path ${line(4)} d="M34 32l-8 10M34 32l8 10M34 50l-8 10M34 50l8 10M34 68l-8 10M34 68l8 10"/><path ${line(3)} d="M74 26v26"/><path ${fill} d="M66 52h16v10H66zM20 80h28v6H20z"/>` },
  // Магазин, булочная, лавка — корзина.
  shop: { ring: "shop", name: "Магазин", svg: `<path ${line(6)} d="M30 42c0-14 8-22 20-22s20 8 20 22"/><path ${fill} d="M14 40h72l-8 40H22z"/><path fill="none" stroke="${PLATE}" stroke-width="4" d="M36 50v22M50 50v22M64 50v22"/>` },
  // Бензоколонка.
  fuel: { ring: "car", name: "Бензоколонка", svg: `<path ${fill} d="M22 18h34v66H22z"/><path fill="${PLATE}" d="M28 26h22v16H28z"/><path ${fill} d="M16 80h46v6H16z"/><path ${line(5)} d="M56 34h10c6 0 8 4 8 9v27c0 4 3 6 6 6s6-2 6-6V44l-8-8"/>` },
  // Мастерская — гаечный ключ.
  service: { ring: "car", name: "Мастерская", svg: `<path ${fill} d="M64 14a20 20 0 0 0-18 27L16 71a8 8 0 0 0 11 11l30-30a20 20 0 0 0 27-18l-12 6-10-6-1-12z"/>` },
};

/** Контур шестерни: `teeth` зубцов между радиусами `inner` и `outer`. */
function gear(cx, cy, outer, inner, teeth) {
  const pts = [];
  for (let k = 0; k < teeth * 4; k++) {
    const a = (k / (teeth * 4)) * Math.PI * 2;
    const r = k % 4 < 2 ? outer : inner;
    pts.push(`${(cx + r * Math.cos(a)).toFixed(1)},${(cy + r * Math.sin(a)).toFixed(1)}`);
  }
  return `M${pts.join("L")}z`;
}

/** Знак `icon` цвета `GLYPH` на прозрачном: яркость маски становится непрозрачностью, так вырезы прозрачны. */
async function glyph(icon) {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="400" height="400" viewBox="0 0 100 100">${icon.svg}</svg>`;
  const { data, info } = await sharp(Buffer.from(svg)).resize(GLYPH_SIZE, GLYPH_SIZE, { kernel: "lanczos3" })
    .ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  for (let i = 0; i < data.length; i += 4) {
    data[i + 3] = Math.round((data[i + 3] * data[i]) / 255);
    [data[i], data[i + 1], data[i + 2]] = GLYPH;
  }
  return sharp(data, { raw: info }).png().toBuffer();
}

const base = await sharp(BADGE).resize(SIZE, SIZE, { kernel: "lanczos3" }).png().toBuffer();
const out = resolve(process.argv[2] ?? resolve(HERE, "../../art/transport/icons"));
await mkdir(out, { recursive: true });
const tiles = [];
for (const [id, icon] of Object.entries(ICONS)) {
  const corner = (SIZE - GLYPH_SIZE) / 2;
  const png = await sharp(base).composite([{ input: await glyph(icon), left: corner, top: corner }]).png().toBuffer();
  await sharp(png).toFile(join(out, `${id}.png`));
  tiles.push({ id, icon, png });
}
// Лист для просмотра: значки в натуральную величину на земле игры и мелко, как в игре издали, с названиями.
const cols = 5, cw = 220, ch = 210;
const rows = Math.ceil(tiles.length / cols);
const layers = [];
for (const [n, { icon, png }] of tiles.entries()) {
  const x = (n % cols) * cw, y = Math.floor(n / cols) * ch;
  layers.push({ input: png, left: x + 20, top: y + 20 });
  layers.push({ input: await sharp(png).resize(36, 36).toBuffer(), left: x + 160, top: y + 60 });
  layers.push({ input: Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="${cw}" height="40"><text x="${cw / 2}" y="26" font-family="Rubik, Arial" font-size="18" fill="#1d242b" text-anchor="middle">${icon.name}</text></svg>`), left: x, top: y + 158 });
}
await sharp({ create: { width: cols * cw, height: rows * ch, channels: 4, background: "#a2ad76" } }).composite(layers).png().toFile(join(out, "sheet.png"));
console.log(`значки — в ${out}`);
