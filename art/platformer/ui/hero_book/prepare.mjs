import { Buffer } from "node:buffer";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const sharp = require("../../../../tools/art/node_modules/sharp");
const path = (relative) => fileURLToPath(new URL(relative, import.meta.url));
const output = (name) => path("../../../../games/platformer/images/" + name + ".png");
const mockup = path("../variants/hero_screen/05_hybrid_camp.png");
const source = await sharp(mockup).removeAlpha().raw().toBuffer({ resolveWithObject: true });
const { width, height, channels } = source.info;
const scale = width / 860;
const spread = Buffer.from(source.data);

// Swatches come from the approved sketch. The coordinates refer to its 860-point layout.
function patch([x, y, w, h], [sx, sy, sw, sh], feather = 1) {
  const dx = Math.round(scale * x), dy = Math.round(scale * y);
  const dw = Math.round(scale * w), dh = Math.round(scale * h);
  const fromX = Math.round(scale * sx), fromY = Math.round(scale * sy);
  const tileW = Math.round(scale * sw), tileH = Math.round(scale * sh);
  const tile = (n, size) => Math.floor(n / size) % 2 ? size - n % size - 1 : n % size;
  for (let j = 0; j < dh; j++) {
    for (let i = 0; i < dw; i++) {
      const old = ((dy + j) * width + dx + i) * channels;
      const sample = ((fromY + tile(j, tileH)) * width + fromX + tile(i, tileW)) * channels;
      const a = feather ? Math.min(1, Math.min(i, j, dw - 1 - i, dh - 1 - j) / (feather * scale)) : 1;
      for (let c = 0; c < channels; c++) {
        spread[old + c] = Math.round(spread[old + c] * (1 - a) + source.data[sample + c] * a);
      }
    }
  }
}

const paper = [706, 67, 47, 15];
for (const rect of [
  [557, 35, 149, 32], [565, 65, 128, 19],
  [621, 115, 187, 18], [621, 135, 187, 17], [621, 153, 187, 17], [621, 171, 187, 23],
  [621, 211, 187, 20], [621, 234, 187, 22], [621, 280, 187, 23], [621, 304, 187, 22],
  [621, 353, 187, 23], [621, 377, 187, 24],
]) patch(rect, paper);
patch([492, 89.5, 115, 12.4], [625, 89.5, 88, 12.4], 0);
patch([510.5, 122.5, 73.5, 31], [565, 153.5, 14, 14], 0.8);
patch([533.5, 151.8, 23, 25], [565, 153.5, 14, 14], 0.8);
patch([520, 209, 64.3, 16], [566, 229, 13, 13], 0.8);
patch([537, 223, 25, 26], [566, 229, 13, 13], 0.8);
patch([524, 278.5, 57, 18.5], [564, 301, 15, 14], 0.8);
patch([539.5, 294.5, 26, 27], [564, 301, 15, 14], 0.8);
patch([521.6, 351, 63, 20], [566, 372, 14, 12], 0.8);
patch([540, 371.5, 24, 22], [566, 372, 14, 12], 0.8);
const leafWidth = 720;
const leafHeight = 810;
const raw = { width, height, channels };
await sharp(spread, { raw }).resize(leafWidth * 2, leafHeight, { fit: "fill" })
  .extract({ left: leafWidth, top: 0, width: leafWidth, height: leafHeight })
  .png().toFile(output("hero_stats_right"));

const leaf = await sharp(path("parchment-leaf-source.png")).removeAlpha()
  .resize(leafWidth, leafHeight, { fit: "fill" }).raw().toBuffer();

// Белая точка относится к бумаге исходного лубка. На новый лист переходит только краска.
async function printIllustration(file, paperWhite, flutePlayer) {
  const { data, info } = await sharp(file).removeAlpha().resize({ width: 618 })
    .raw().toBuffer({ resolveWithObject: true });
  const page = Buffer.from(leaf);
  const left = 66;
  const top = 80;
  for (let y = 0; y < info.height; y++) {
    const ny = Math.abs(2 * (y + 0.5) / info.height - 1);
    for (let x = 0; x < info.width; x++) {
      const nx = Math.abs(2 * (x + 0.5) / info.width - 1);
      const radius = nx ** 3.2 + ny ** 3.2;
      const edge = Math.max(0, Math.min(1, (1 - radius) / 0.36));
      let coverage = edge * edge * (3 - 2 * edge);
      if (flutePlayer) {
        const headDistance = ((x / info.width - 0.17) / 0.077) ** 2 + ((y / info.height - 0.14) / 0.19) ** 2;
        coverage = Math.max(coverage, Math.max(0, Math.min(1, (1 - headDistance) * 3)));
      }
      const pixel = (y * info.width + x) * 3;
      const dest = ((y + top) * leafWidth + x + left) * 3;
      for (let channel = 0; channel < 3; channel++) {
        const density = Math.max(0, 1 - data[pixel + channel] / paperWhite[channel]);
        page[dest + channel] = Math.round(leaf[dest + channel] * (1 - density * coverage));
      }
    }
  }
  return sharp(page, { raw: { width: leafWidth, height: leafHeight, channels: 3 } });
}

for (const [hero, sourceName, paperWhite] of [
  ["ilya", "ilya_camp_lubok_v1", [248, 199, 113]],
  ["alyosha", "alyosha_flute_lubok_v2", [248, 199, 111]],
  ["vasilisa", "vasilisa_pond_lubok_v1", [247, 192, 104]],
]) {
  const page = await printIllustration(path("../../hero/" + sourceName + ".png"), paperWhite, hero === "alyosha");
  await page.png().toFile(output("book_" + hero));
}

// Оттиски из эскиза видны в пустых ячейках снаряжения.
for (const [slot, x, y, w, h] of [
  ["head", 64, 213.4, 41, 43], ["body", 135, 214.4, 51, 43],
  ["gloves", 210, 216, 48, 41], ["belt", 281, 219.5, 46, 37],
  ["boots", 350, 217, 47, 40], ["offhand", 142, 273, 40, 47],
  ["amulet", 215, 271.8, 37, 46], ["ring", 285, 277.5, 39, 39],
]) {
  const { data, info } = await sharp(mockup).extract({
    left: Math.round(x * scale), top: Math.round(y * scale),
    width: Math.round(w * scale), height: Math.round(h * scale),
  }).removeAlpha().raw().toBuffer({ resolveWithObject: true });
  const stamp = Buffer.alloc(info.width * info.height * 4);
  for (let i = 0; i < info.width * info.height; i++) {
    stamp[i * 4] = 105;
    stamp[i * 4 + 1] = 58;
    stamp[i * 4 + 2] = 28;
    stamp[i * 4 + 3] = Math.round(Math.max(0, Math.min(1, (224 - data[i * 3]) / 140)) * 205);
  }
  await sharp(stamp, { raw: { width: info.width, height: info.height, channels: 4 } })
    .resize(114, 98, { fit: "contain", background: "#00000000" }).png().toFile(output("book_mark_" + slot));
}

const sheet = path("items-source.png");
const icons = ["axe", "dagger", "staff", "head", "body", "gloves", "belt", "boots", "offhand", "amulet", "ring"];
const meta = await sharp(sheet).metadata();
for (const [i, icon] of icons.entries()) {
  const col = i % 4, row = Math.floor(i / 4);
  const left = Math.round(meta.width * col / 4), top = Math.round(meta.height * row / 3);
  const cell = await sharp(sheet).extract({
    left, top, width: Math.round(meta.width * (col + 1) / 4) - left,
    height: Math.round(meta.height * (row + 1) / 3) - top,
  }).png().toBuffer();
  const trimmed = await sharp(cell).trim({ background: "#00000000", threshold: 24 }).png().toBuffer();
  await sharp(trimmed).resize(96, 96, { fit: "contain", background: "#00000000" }).png().toFile(output("item_" + icon));
}
