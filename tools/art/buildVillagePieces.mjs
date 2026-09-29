// Обработка нарисованных кусков деревни («Фаза-02-5-первая-нарисованная-локация», пункты 3–8):
// снимает однотонный фон (розовый — у всех кусков, кроме травы), обрезает по содержимому,
// приводит к масштабу 96 точек на клетку и гасит концы кусков земли. Исходники — `art/rpg/`,
// результат — `games/rpg/images/`. Стены после этого ещё режутся на полоски отдельной программой
// (`web/scripts/buildRpgVillageScene.mjs`, требование 13) — здесь они только приведены к масштабу,
// без резки. Детерминирована: повторный запуск даёт те же байты. Запуск: `node
// tools/art/buildVillagePieces.mjs` из `tools/art/` (нужен `npm install` там же).

import { writeFileSync, mkdirSync, rmSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import sharp from "sharp";
import { cutOut, fadeEnds, removeBackground, visibleBox, VISIBLE_ALPHA } from "./sheet.mjs";
import { blendWithHalfShift } from "./tile.mjs";
import { PIECES } from "../../web/scripts/rpg-village/catalog.mjs";

const scriptDir = fileURLToPath(new URL(".", import.meta.url));
const artDir = resolve(scriptDir, "../../art/rpg");
const guidesDir = resolve(artDir, "guides");
const outDir = resolve(scriptDir, "../../games/rpg/images");
// Требование 7: необрезанные стены (`_raw.png`, режет на полоски `buildRpgVillageScene.mjs`) — не
// готовый кусок игры, в образ им попадать незачем. Временная папка вне `games/`, в `.gitignore`.
const rawOutDir = resolve(scriptDir, ".build");

// Требование 4: заготовки — розовое поле 1600×900 с масштабом 150 точек заготовки на клетку
// (`wall_guide.png`: полоса 1200 точек = 8 клеток; `palisade_guide.png`: 900 точек = 6 клеток).
export const GUIDE_PX_PER_CELL = 150;

// Требование: исходники 1280×720 — заготовка 1600×900, уменьшенная в 0,8 раза, то есть 120 точек
// исходника на клетку; масштаб к игровым 96 точкам на клетку — тот же множитель 0,8.
const SOURCE_TO_GAME_SCALE = 0.8;

const FADE_PX = 48; // требование 7: полклетки на игровом масштабе (96 точек на клетку)

// Требование 5: у кусков без заготовки (валун, куст, столб, ель, берёза) масштаб задаёт не общий
// множитель 0,8 (тот держится на заготовке — требование 4, здесь её нет), а целевой размер в
// клетках из каталога (`targetSize`), по ширине или высоте картинки после обрезки по содержимому.
export function targetHeightFor(box, targetSize) {
  if (!targetSize) return Math.round(box.height * SOURCE_TO_GAME_SCALE);
  const axisPx = targetSize.axis === "width" ? box.width : box.height;
  const scale = (targetSize.cells * 96) / axisPx;
  return Math.round(box.height * scale);
}

async function processGeneric(name, { fade = false, outputName = name, targetSize = null, destDir = outDir } = {}) {
  const image = await removeBackground(resolve(artDir, `${name}.png`), "magenta");
  const box = visibleBox([image]);
  const targetHeight = targetHeightFor(box, targetSize);
  const { data, info } = await cutOut(image, box, targetHeight);
  if (!fade) {
    writeFileSync(resolve(destDir, `${outputName}.png`), data);
    console.log(`${outputName}.png: ${info.width}×${info.height}`);
    return;
  }
  const raw = await sharp(data).raw().toBuffer();
  fadeEnds(raw, info.width, info.height, FADE_PX);
  await sharp(raw, { raw: { width: info.width, height: info.height, channels: 4 } }).png().toFile(resolve(destDir, `${outputName}.png`));
  console.log(`${outputName}.png: ${info.width}×${info.height} (концы погашены на ${FADE_PX})`);
}

// Требование: ширина серой «коробки» заготовки в точках — единственный ориентир, который эта
// коробка даёт (высота и глубина коробки для масштаба не нужны, дом ставится по ширине основания).
// Точка коробки — почти нейтральный серый (для розового фона разброс каналов огромен).
export async function guideBoxWidthPx(guidePath) {
  const { data, info } = await sharp(guidePath).removeAlpha().raw().toBuffer({ resolveWithObject: true });
  let minX = info.width;
  let maxX = -1;
  for (let y = 0; y < info.height; y++) {
    for (let x = 0; x < info.width; x++) {
      const i = (y * info.width + x) * 3;
      const r = data[i];
      const g = data[i + 1];
      const b = data[i + 2];
      const spread = Math.max(r, g, b) - Math.min(r, g, b);
      if (spread > 12) continue; // розовый фон — не серый
      minX = Math.min(minX, x);
      maxX = Math.max(maxX, x);
    }
  }
  if (maxX < 0) throw new Error(`${guidePath}: в заготовке не нашлось серой коробки`);
  return maxX - minX + 1;
}

function rowWidth(rgba, width, y) {
  let minX = width;
  let maxX = -1;
  for (let x = 0; x < width; x++) {
    if (rgba[(y * width + x) * 4 + 3] < VISIBLE_ALPHA) continue;
    minX = Math.min(minX, x);
    maxX = Math.max(maxX, x);
  }
  return maxX >= 0 ? maxX - minX + 1 : 0;
}

// Требование: у smithy.png/chertog.png (1024×1024, не из ряда 1280×720) масштаб — по ширине
// основания дома против ширины коробки собственной заготовки, а не общий множитель 0,8.
//
// Основание — широкий каменный цоколь дома в нижней четверти картинки. Самые нижние точки — не
// он: под цоколем в кадре ещё остаётся мелкая деталь переднего плана (наковальня, ступени
// крыльца) заметно более узкого силуэта, а отдельные строки внутри самого цоколя чуть скачут
// из-за резьбы и теней. Медиана ширины непрозрачной полосы по строкам нижней четверти, без
// последних 3% высоты (сама мелкая деталь), гасит и то и другое.
export function baseWidthPx(rgba, width, height) {
  const marginRows = Math.round(height * 0.03);
  const bandStart = Math.round(height * 0.75);
  const bandEnd = height - marginRows;
  const widths = [];
  for (let y = bandStart; y < bandEnd; y++) {
    const w = rowWidth(rgba, width, y);
    if (w > 0) widths.push(w);
  }
  if (widths.length === 0) throw new Error("в нижней четверти куска нет непрозрачных точек — основание не найдено");
  widths.sort((a, b) => a - b);
  return widths[Math.floor(widths.length / 2)];
}

async function processBuilding(name) {
  const image = await removeBackground(resolve(artDir, `${name}.png`), "magenta");
  const box = visibleBox([image]);
  const { data: croppedPng } = await cutOut(image, box, undefined);
  const { data: croppedRaw, info: croppedInfo } = await sharp(croppedPng).raw().toBuffer({ resolveWithObject: true });
  const base = baseWidthPx(croppedRaw, croppedInfo.width, croppedInfo.height);
  const boxPx = await guideBoxWidthPx(resolve(guidesDir, `${name}_guide.png`));
  const boxCells = boxPx / GUIDE_PX_PER_CELL;
  const scale = (boxCells * 96) / base;
  const targetHeight = Math.round(croppedInfo.height * scale);
  await sharp(croppedPng).resize({ height: targetHeight, kernel: "lanczos3" }).png().toFile(resolve(outDir, `${name}.png`));
  console.log(`${name}.png: основание ${base} точек = ${boxCells.toFixed(3)} клетки по заготовке, масштаб ${scale.toFixed(3)}, итог ${targetHeight * (croppedInfo.width / croppedInfo.height)}×${targetHeight}`);
}

async function processGrass() {
  const cells = 4;
  const cellPx = 96;
  const size = cells * cellPx;
  const { data } = await sharp(resolve(artDir, "grass.png")).resize(size, size, { kernel: "lanczos3" }).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  const seamless = blendWithHalfShift(blendWithHalfShift(data, size, size, 0), size, size, 1);
  await sharp(seamless, { raw: { width: size, height: size, channels: 4 } }).png().toFile(resolve(outDir, "grass.png"));
  console.log(`grass.png: ${size}×${size} (бесшовная плитка ${cells}×${cells})`);
}

const isMainModule = import.meta.url === pathToFileURL(process.argv[1] ?? "").href;
if (isMainModule) {
  mkdirSync(outDir, { recursive: true });
  rmSync(rawOutDir, { recursive: true, force: true });
  mkdirSync(rawOutDir, { recursive: true });

  await processGrass();

  // Стены пишутся с суффиксом `_raw` во временную папку (требование 7): их ещё режет на полоски
  // `buildRpgVillageScene.mjs` (`web/scripts/rpg-village/catalog.mjs` поясняет, зачем два разных
  // пути) — необрезанный кусок в игру не идёт.
  for (const name of ["wall_log", "wall_palisade", "wall_stone"]) {
    await processGeneric(name, { outputName: `${name}_raw`, destDir: rawOutDir });
  }
  for (const name of ["izba"]) {
    await processGeneric(name);
  }
  for (const name of ["post", "spruce", "birch", "boulder", "bush"]) {
    await processGeneric(name, { targetSize: PIECES[name].targetSize });
  }
  for (const name of ["path_straight", "path_turn", "stream", "stream_bridge"]) {
    await processGeneric(name, { fade: true });
  }
  for (const name of ["smithy", "chertog"]) {
    await processBuilding(name);
  }

  console.log("куски деревни обработаны");
}
