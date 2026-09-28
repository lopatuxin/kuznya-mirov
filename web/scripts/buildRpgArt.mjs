// Сборка графики ролевой игры из исходников LPC («Фаза-02-враг-на-пути», пункты 1–8): листы
// героя, гоблина и орка из скачанных частей генератора персонажей (основной лист 9×5 и отдельный
// лист удара), набор плиток земли и картинки стен/деревьев/камня из LPC Tile Atlas, файл авторов.
// Сеть скрипту не нужна — все исходники уже лежат в `rpg-lpc/`. Запуск: `node scripts/buildRpgArt.mjs`
// из `web/`. Результат детерминирован: повторный запуск даёт те же байты (пункт 2).

import { PNG } from "pngjs";
import { existsSync, mkdirSync, readdirSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { hexToRgb } from "./generateDemoImages.mjs";
import { parseLocationMap, extractRectangles, CELL } from "./rpgLocationMap.mjs";
import {
  TERRAIN_COLUMNS,
  TERRAIN_FRAME_COUNT,
  NEIGHBOR_BIT,
  pathTileIndex,
  waterTileIndex,
  GRASS_VARIANT_TILES,
  DECORATION_TILES,
} from "./rpgTerrainTiles.mjs";

const scriptDir = fileURLToPath(new URL(".", import.meta.url));
const lpcDir = resolve(scriptDir, "rpg-lpc");
const gameDir = resolve(scriptDir, "../../games/rpg");
const imagesDir = resolve(gameDir, "images");

const FRAME = 64; // кадр тела в исходниках генератора LPC
const MAIN_COLUMNS = 9;
const MAIN_ROWS = 5; // требование 5: строки 0–3 — ходьба, строка 4 — падение
const ATTACK_COLUMNS = 6;
const ATTACK_ROWS = 4;
const TILE = 32; // клетка сцены — требование 6 фазы и «32 точки на клетку» фазы 13 Кузни

// ---------------------------------------------------------------------------------------------
// PNG-примитивы поверх pngjs: чтение готового файла и создание пустого RGBA-полотна.
// ---------------------------------------------------------------------------------------------

function readPng(path) {
  return PNG.sync.read(readFileSync(path));
}

function makeCanvas(width, height) {
  const png = new PNG({ width, height });
  png.data.fill(0);
  return png;
}

function writePngFile(path, png) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, PNG.sync.write(png));
}

// «src over dst» — обычное альфа-смешение; полностью непрозрачный и полностью прозрачный
// источник — частые случаи в пиксель-арте, разбираем их без деления, чтобы не терять точность.
export function compositeRegion(dest, sw, sh, src, srcX, srcY, destX, destY) {
  for (let y = 0; y < sh; y++) {
    for (let x = 0; x < sw; x++) {
      const srcIdx = ((srcY + y) * src.width + (srcX + x)) * 4;
      const a = src.data[srcIdx + 3];
      if (a === 0) continue;
      const destIdx = ((destY + y) * dest.width + (destX + x)) * 4;
      if (a === 255) {
        dest.data[destIdx] = src.data[srcIdx];
        dest.data[destIdx + 1] = src.data[srcIdx + 1];
        dest.data[destIdx + 2] = src.data[srcIdx + 2];
        dest.data[destIdx + 3] = 255;
        continue;
      }
      const da = dest.data[destIdx + 3];
      const outA = a + (da * (255 - a)) / 255;
      if (outA === 0) continue;
      for (let c = 0; c < 3; c++) {
        const blended = (src.data[srcIdx + c] * a + dest.data[destIdx + c] * da * (1 - a / 255)) / outA;
        dest.data[destIdx + c] = Math.round(blended);
      }
      dest.data[destIdx + 3] = Math.round(outA);
    }
  }
}

// Как `compositeRegion`, но точки источника, близкие к `keyColor` (в пределах 6 на канал —
// допуск ±1, что и у `recolorSkin`, взятый шире из-за сглаживания на краю вырезки), пропускаются
// как прозрачные — источник вырезан вместе с фоном травы, не отдельным спрайтом на своей
// прозрачности; так мелочь ложится только своими точками, а не всем прямоугольником вырезки
// (требование 6, ревью: «швы оттенков»). Только для мака — используется внутри этого файла.
const KEY_TOLERANCE = 6;
function compositeRegionKeyed(dest, sw, sh, src, srcX, srcY, destX, destY, keyColor) {
  const [kr, kg, kb] = keyColor;
  for (let y = 0; y < sh; y++) {
    for (let x = 0; x < sw; x++) {
      const srcIdx = ((srcY + y) * src.width + (srcX + x)) * 4;
      const r = src.data[srcIdx];
      const g = src.data[srcIdx + 1];
      const b = src.data[srcIdx + 2];
      if (Math.abs(r - kr) <= KEY_TOLERANCE && Math.abs(g - kg) <= KEY_TOLERANCE && Math.abs(b - kb) <= KEY_TOLERANCE) continue;
      compositeRegion(dest, 1, 1, src, srcX + x, srcY + y, destX + x, destY + y);
    }
  }
}

export { hexToRgb };

// ---------------------------------------------------------------------------------------------
// Перекраска кожи/волос по таблицам цветов генератора LPC — читаются из файлов в `rpg-lpc/palettes/`
// (`tools/palettes/ulpc-body-palettes.json` и `ulpc-hair-palettes.json` генератора на закреплённом
// коммите), а не заданы строками в коде (ревью фазы: «таблицы цветов... скрипт читает их»).
// PALETTE_RECOLOR_GUIDE.md генератора: совпадение по каналам с допуском ±1.
// ---------------------------------------------------------------------------------------------

const palettesDir = resolve(lpcDir, "palettes");
const bodyPalettes = JSON.parse(readFileSync(resolve(palettesDir, "ulpc-body-palettes.json"), "utf8"));
const hairPalettes = JSON.parse(readFileSync(resolve(palettesDir, "ulpc-hair-palettes.json"), "utf8"));

const RECOLOR_PALETTES = {
  // Требование 4: кожа гоблина и орка зелёная.
  green: { source: bodyPalettes.source, target: bodyPalettes.green },
  // «Волосы» героя приходят из хранилища рыжими («orange» — вариант по умолчанию); требование 4
  // («тёмные волосы») перекрашивает их в тёмно-каштановый той же таблицей цветов генератора.
  hairDark: { source: hairPalettes.orange, target: hairPalettes.dark_brown },
};

export function recolorSkin(png, sourceHexes, targetHexes, tolerance = 1) {
  const sources = sourceHexes.map(hexToRgb);
  const targets = targetHexes.map(hexToRgb);
  for (let i = 0; i < png.data.length; i += 4) {
    if (png.data[i + 3] === 0) continue;
    const r = png.data[i];
    const g = png.data[i + 1];
    const b = png.data[i + 2];
    for (let p = 0; p < sources.length; p++) {
      const [sr, sg, sb] = sources[p];
      if (Math.abs(r - sr) <= tolerance && Math.abs(g - sg) <= tolerance && Math.abs(b - sb) <= tolerance) {
        const [tr, tg, tb] = targets[p];
        png.data[i] = tr;
        png.data[i + 1] = tg;
        png.data[i + 2] = tb;
        break;
      }
    }
  }
  return png;
}

function applyRecolor(png, recolorName) {
  const palette = RECOLOR_PALETTES[recolorName];
  recolorSkin(png, palette.source, palette.target);
  return png;
}

// ---------------------------------------------------------------------------------------------
// Части персонажей: «простая» часть (тело, голова, волосы, торso, ноги, стопы, кинжал гоблина) —
// три файла `walk.png` (9×4 кадра 64×64), `hurt.png` (6×1) и, если есть, `slash.png` (6×4) в её
// каталоге. «Оружие с оверсайз-взмахом» (меч героя, булава орка) — своя пара файлов на каждую
// анимацию, «лицевая» (перед телом) и «изнаночная» (за телом): `front_walk`/`behind_walk`,
// `front_hurt`/`behind_hurt`, `front_attack`/`behind_attack`. Откуда взяты и почему — см.
// `sources.json` и отчёт: генератор рисует взмах мечом/булавой в кадре крупнее 64 (128/192),
// смещая копию обычных 64-точечных кадров тела на центр большего кадра (`sources/canvas/
// draw-frames.ts` генератора: `drawFrameToFrame`, `offset = (destFrameSize - srcFrameSize) / 2`).
// ---------------------------------------------------------------------------------------------

function loadSimpleParts(dir) {
  const base = resolve(lpcDir, dir);
  const slashPath = resolve(base, "slash.png");
  return {
    walk: readPng(resolve(base, "walk.png")),
    hurt: readPng(resolve(base, "hurt.png")),
    slash: existsSync(slashPath) ? readPng(slashPath) : null,
  };
}

function loadWeaponParts(dir) {
  const base = resolve(lpcDir, dir);
  return {
    frontWalk: readPng(resolve(base, "front_walk.png")),
    behindWalk: readPng(resolve(base, "behind_walk.png")),
    frontHurt: readPng(resolve(base, "front_hurt.png")),
    behindHurt: readPng(resolve(base, "behind_hurt.png")),
    frontAttack: readPng(resolve(base, "front_attack.png")),
    behindAttack: readPng(resolve(base, "behind_attack.png")),
  };
}

// Основной лист (требование 5): 9×5 кадров 64×64 — строки 0–3 ходьба (вверх/влево/вниз/вправо,
// 9 кадров), строка 4 падение (6 кадров, столбцы 6–8 остаются прозрачными). Слои — по возрастанию
// «приоритета» генератора (zPos): body 10 < head 20 < hair 30 < torso 60 < legs 70 < feet 80,
// оружие — «изнаночный» слой (zPos ~9, перед телом ничего не рисует, кроме краешка за спиной) ниже
// тела, «лицевой» (zPos ~140) выше всех.
export function buildMainSheet(layers) {
  const sheet = makeCanvas(FRAME * MAIN_COLUMNS, FRAME * MAIN_ROWS);
  const entries = [];
  for (const layer of layers) {
    if (layer.kind === "weapon") {
      const parts = loadWeaponParts(layer.dir);
      entries.push({ zPos: layer.zPosBehind, walk: parts.behindWalk, hurt: parts.behindHurt });
      entries.push({ zPos: layer.zPosFront, walk: parts.frontWalk, hurt: parts.frontHurt });
      continue;
    }
    const parts = loadSimpleParts(layer.dir);
    if (layer.recolor) {
      applyRecolor(parts.walk, layer.recolor);
      applyRecolor(parts.hurt, layer.recolor);
    }
    entries.push({ zPos: layer.zPos, walk: parts.walk, hurt: parts.hurt });
  }
  entries.sort((a, b) => a.zPos - b.zPos);
  for (const entry of entries) {
    compositeRegion(sheet, FRAME * 9, FRAME * 4, entry.walk, 0, 0, 0, 0);
    compositeRegion(sheet, FRAME * 6, FRAME, entry.hurt, 0, 0, 0, FRAME * 4);
  }
  return sheet;
}

// Лист удара (требование 5): 6×4 кадра стороны `frameSize`. Часть тела даёт кадр 64×64 (её
// `slash.png`), центрированный в кадре большего размера тем же смещением, что вычисляет
// генератор (`drawFrameToFrame`); оружие с готовым оверсайз-взмахом (меч, булава) уже нарисовано
// в кадре ровно `frameSize`, копируется без смещения. Части без своего `slash.png` (нет у нашего
// набора — заглушка на случай будущей части без взмаха) пропускаются.
export function buildAttackSheet(frameSize, layers) {
  const sheet = makeCanvas(frameSize * ATTACK_COLUMNS, frameSize * ATTACK_ROWS);
  const entries = [];
  for (const layer of layers) {
    if (layer.kind === "weapon") {
      const parts = loadWeaponParts(layer.dir);
      entries.push({ zPos: layer.zPosBehind, attack: parts.behindAttack });
      entries.push({ zPos: layer.zPosFront, attack: parts.frontAttack });
      continue;
    }
    const parts = loadSimpleParts(layer.dir);
    if (!parts.slash) continue;
    if (layer.recolor) applyRecolor(parts.slash, layer.recolor);
    entries.push({ zPos: layer.zPos, attack: parts.slash });
  }
  entries.sort((a, b) => a.zPos - b.zPos);
  for (const entry of entries) {
    const srcFrame = entry.attack.width / ATTACK_COLUMNS;
    const offset = (frameSize - srcFrame) / 2;
    for (let row = 0; row < ATTACK_ROWS; row++) {
      for (let col = 0; col < ATTACK_COLUMNS; col++) {
        compositeRegion(
          sheet,
          srcFrame,
          srcFrame,
          entry.attack,
          col * srcFrame,
          row * srcFrame,
          col * frameSize + offset,
          row * frameSize + offset,
        );
      }
    }
  }
  return sheet;
}

// Требование 4: слои по персонажу, в порядке слоя генератора (см. комментарий выше). Требование
// 4 (оружие): герой — одноручный меч (arming sword, взмах в кадре 128×128 — `sources.json`
// `weapon/sword/arming`); орк — булава (mace, взмах в кадре 192×192 — топор одной рукой того же
// хранилища взмаха ≤128 не имеет, см. отчёт); гоблин — кинжал (взмах уже в кадре 64×64, простая
// часть). `attackFrame` — сторона кадра листа удара этого персонажа.
export const CHARACTERS = {
  hero: {
    attackFrame: 128,
    layers: [
      { kind: "simple", dir: "body/male", zPos: 10 },
      { kind: "simple", dir: "head/human_male", zPos: 20 },
      { kind: "simple", dir: "hair/plain_adult", zPos: 30, recolor: "hairDark" },
      { kind: "simple", dir: "torso/leather_male", zPos: 60 },
      { kind: "simple", dir: "legs/pants_male", zPos: 70 },
      { kind: "simple", dir: "feet/boots_male", zPos: 80 },
      { kind: "weapon", dir: "weapon/arming_sword", zPosBehind: 9, zPosFront: 140 },
    ],
  },
  goblin: {
    attackFrame: 64,
    layers: [
      { kind: "simple", dir: "body/teen", zPos: 10, recolor: "green" },
      { kind: "simple", dir: "head/goblin_adult", zPos: 20, recolor: "green" },
      { kind: "simple", dir: "legs/shorts_male", zPos: 70 },
      { kind: "simple", dir: "weapon/dagger", zPos: 100 },
    ],
  },
  orc: {
    attackFrame: 192,
    layers: [
      { kind: "simple", dir: "body/muscular", zPos: 10, recolor: "green" },
      { kind: "simple", dir: "head/orc_male", zPos: 20, recolor: "green" },
      { kind: "simple", dir: "torso/leather_male", zPos: 60 },
      { kind: "weapon", dir: "weapon/mace", zPosBehind: 9, zPosFront: 140 },
    ],
  },
};

// Требование 8: основной лист — `size` в клетках 2×2 (кадр 64 при 32 точки на клетку), `anchor:
// "bottom"` со сдвигом, при котором ступни стоят на нижнем крае. Сдвиг измерен по кадру «стоя,
// лицом вниз» (body/male, body/teen, body/muscular): нижняя точка стопы — пиксель 61–62 из 64,
// то есть на 2 точки (0,0625 клетки) выше нижнего края кадра.
export const CHARACTER_IMAGE_SIZE = [2, 2];
const FOOT_GAP_PX = 2;
export const CHARACTER_IMAGE_OFFSET = [0, FOOT_GAP_PX / TILE];

// Лист удара — тело центрировано в большем кадре (padding сверху и снизу поровну), поэтому ступни
// стоят ниже на ту же добавку: `size` — сторона кадра в клетках, `offset` — тот же зазор 2 точки
// плюс половина добавленного паддинга, тем же способом, что и сдвиг основного листа.
export function attackImageSize(frameSize) {
  const cells = frameSize / TILE;
  return [cells, cells];
}

export function attackImageOffset(frameSize) {
  const paddingPx = (frameSize - FRAME) / 2;
  return [0, (paddingPx + FOOT_GAP_PX) / TILE];
}

// ---------------------------------------------------------------------------------------------
// Набор плиток земли: автотайл по битовой маске соседей (rpgTerrainTiles.mjs) поверх реальных
// кусков текстуры из LPC Tile Atlas — трава, тропинка (мощение), вода с берегом-каймой. Форма
// плитки — прямоугольник с отступами по сторонам, где отступ обнулён у связанной стороны и
// скруглён по четверти окружности там, где обе соседние стороны отступают (внешний угол пятна);
// внутренние (вогнутые) углы поворотов коридора получаются сами, без отдельной картинки на
// каждый случай. Атлас читается один раз за сборку (`getAtlas`), а не при каждом обращении.
// ---------------------------------------------------------------------------------------------

const ATLAS_PATH = resolve(lpcDir, "atlas/terrain_atlas.png");
let atlasCache = null;
function getAtlas() {
  if (!atlasCache) atlasCache = readPng(ATLAS_PATH);
  return atlasCache;
}

// Требование 6: базовый слой — настоящая текстурная трава LPC Tile Atlas, не ровная заливка
// (ревью): `grassA` и `grassB` — два выреза одного и того же мелкого травяного узора (тонкие
// стебли на просвет): узор двухцветный и мелкий, поэтому стык двух копий на глаз не виден, хотя
// края вырезки совпадают не точно; близки по среднему тону (±1 на канал), но заметно различаются
// рисунком (~40% точек расходятся между вырезками, разный узор стеблей).
// Ревью: прежний `water` {306,390} был вырезан из середины ОДНОГО пруда-блоба — соседние клетки
// каждая заново копировали тот же кадр, но волновые полосы там текут к берегу под разным углом
// в разных точках блоба, так что стык двух копий давал видимый шов (и цеплял край берега). `water`
// {672,544} — открытая водная гладь с рябью, которая сама по себе повторяется по кругу: проверено
// сдвигом на 32 по обеим осям — шва нет (см. тест бесшовности воды, как у травы).
const ATLAS_SWATCH = {
  grassA: { x: 24, y: 728 },
  grassB: { x: 40, y: 744 },
  path: { x: 256, y: 640 },
  water: { x: 672, y: 544 },
  shore: { x: 256, y: 672 },
};
const BRICK_SWATCH = { x: 672, y: 705 };
// Верх стены — верхний край блока серой стены развалин из атласа (ревью: прежняя {40,540} давала
// шов через каждые 32 точки и захватывала обрывок пучка травы); у этой вырезки крайние столбцы и
// строки совпадают, поэтому при повторе через 32 точки шва нет (см. тест), чужих вкраплений нет.
const WALL_TOP_SWATCH = { x: 672, y: 832 };
// Требование 6: цветок среди мелочей — маленький цветок со стеблем, вырезан вместе с фоном травы
// (47,129,54 — фон именно этой конкретной вырезки исходника, не текущих `grassA`/`grassB`);
// мелочи кладутся на прозрачный кадр (см. `rpgTerrainTiles.mjs`), поэтому фон вырезки маскируется
// этим же цветом — ключом прозрачности — при композиции (`compositeRegionKeyed`), а не остаётся
// сплошным прямоугольником.
const FLOWER_BBOX = { x: 740, y: 371, w: 12, h: 13 };
const FLOWER_BACKGROUND = [47, 129, 54];
// Требование 6: камешек и пучок травы среди мелочей — настоящие маленькие вырезки со своей
// прозрачностью (не кусок середины большого камня, ревью): камешек — отдельный обломок рядом с
// валуном `ROCK_BBOX`, пучок — основание куста рогоза (без его тёмных метёлок).
const ROCK_DECORATION_BBOX = { x: 930, y: 886, w: 11, h: 8 };
const TUFT_BBOX = { x: 835, y: 962, w: 22, h: 20 };
const TREE_BBOX = { x: 929, y: 902, w: 95, h: 117 };
const ROCK_BBOX = { x: 866, y: 852, w: 58, h: 41 };

function tileFrom(atlas, swatch) {
  const canvas = makeCanvas(TILE, TILE);
  compositeRegion(canvas, TILE, TILE, atlas, swatch.x, swatch.y, 0, 0);
  return canvas;
}

// Заполняет прямоугольник `w × h` внутри `dest`, замощая исходный кусок `TILE × TILE` по кругу —
// для стен и мелких плиток произвольного размера из одного и того же образца текстуры.
function tileFill(dest, destX, destY, w, h, swatchCanvas) {
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const sx = x % TILE;
      const sy = y % TILE;
      compositeRegion(dest, 1, 1, swatchCanvas, sx, sy, destX + x, destY + y);
    }
  }
}

const AUTOTILE_MARGIN = 8;
const AUTOTILE_CORNER = 8;

function autotileInsets(bitmask) {
  return {
    top: bitmask & NEIGHBOR_BIT.N ? 0 : AUTOTILE_MARGIN,
    right: bitmask & NEIGHBOR_BIT.E ? 0 : AUTOTILE_MARGIN,
    bottom: bitmask & NEIGHBOR_BIT.S ? 0 : AUTOTILE_MARGIN,
    left: bitmask & NEIGHBOR_BIT.W ? 0 : AUTOTILE_MARGIN,
  };
}

// Точка внутри «пятна»: прямоугольник с отступами `insets`, углы, где отступают ОБЕ соседние
// стороны, срезаны по четверти окружности радиуса `AUTOTILE_CORNER` — стандартный «блочный»
// автотайл по 4 соседям (без диагоналей), поворотам ничего специально рисовать не нужно.
export function insideAutotileBlob(x, y, insets) {
  const left = insets.left;
  const right = TILE - insets.right;
  const top = insets.top;
  const bottom = TILE - insets.bottom;
  if (x < left || x >= right || y < top || y >= bottom) return false;

  const nearLeft = insets.left > 0 && x < left + AUTOTILE_CORNER;
  const nearRight = insets.right > 0 && x >= right - AUTOTILE_CORNER;
  const nearTop = insets.top > 0 && y < top + AUTOTILE_CORNER;
  const nearBottom = insets.bottom > 0 && y >= bottom - AUTOTILE_CORNER;

  function cornerCut(cx, cy) {
    const dx = x - cx;
    const dy = y - cy;
    return dx * dx + dy * dy > AUTOTILE_CORNER * AUTOTILE_CORNER;
  }
  if (nearTop && nearLeft && cornerCut(left + AUTOTILE_CORNER - 1, top + AUTOTILE_CORNER - 1)) return false;
  if (nearTop && nearRight && cornerCut(right - AUTOTILE_CORNER, top + AUTOTILE_CORNER - 1)) return false;
  if (nearBottom && nearLeft && cornerCut(left + AUTOTILE_CORNER - 1, bottom - AUTOTILE_CORNER)) return false;
  if (nearBottom && nearRight && cornerCut(right - AUTOTILE_CORNER, bottom - AUTOTILE_CORNER)) return false;
  return true;
}

// Плитка остаётся прозрачной везде, кроме тропинки/берега/воды (ревью: «швы оттенков») — слой
// травы уже лежит под ней в `ground` (требование 15), закрашивать фон второй раз не нужно, и он
// не заслонит собой не тот вариант травы, если под этой клеткой лежит `grassB`, а не `grassA`.
function drawAutotileTile(dest, destX, destY, bitmask, featureSwatch, shoreSwatch) {
  const insets = autotileInsets(bitmask);
  if (shoreSwatch) {
    const shoreInsets = {
      top: Math.max(0, insets.top - 4),
      right: Math.max(0, insets.right - 4),
      bottom: Math.max(0, insets.bottom - 4),
      left: Math.max(0, insets.left - 4),
    };
    for (let y = 0; y < TILE; y++) {
      for (let x = 0; x < TILE; x++) {
        if (insideAutotileBlob(x, y, shoreInsets)) {
          compositeRegion(dest, 1, 1, shoreSwatch, x % TILE, y % TILE, destX + x, destY + y);
        }
      }
    }
  }
  for (let y = 0; y < TILE; y++) {
    for (let x = 0; x < TILE; x++) {
      if (insideAutotileBlob(x, y, insets)) {
        compositeRegion(dest, 1, 1, featureSwatch, x % TILE, y % TILE, destX + x, destY + y);
      }
    }
  }
}

// Камешек — маленький обломок камня, вырезан по форме на своей прозрачности (ревью: не
// прямоугольник из середины валуна), требование 6 «мелочи поверх травы», один кадр на прозрачном
// фоне — трава под ней видна сквозь прозрачное, какой бы вариант ни лежал в клетке.
function drawRockDecoration(dest, destX, destY, atlas) {
  compositeRegion(dest, ROCK_DECORATION_BBOX.w, ROCK_DECORATION_BBOX.h, atlas, ROCK_DECORATION_BBOX.x, ROCK_DECORATION_BBOX.y, destX + 10, destY + 12);
}

// Цветок — та же идея, но вырезка `FLOWER_BBOX` держит фон вместе с собой (не на своей
// прозрачности), поэтому фон маскируется по цвету при композиции (`compositeRegionKeyed`).
function drawFlowerDecoration(dest, destX, destY, atlas) {
  compositeRegionKeyed(dest, FLOWER_BBOX.w, FLOWER_BBOX.h, atlas, FLOWER_BBOX.x, FLOWER_BBOX.y, destX + 10, destY + 10, FLOWER_BACKGROUND);
}

// Пучок травы — основание куста рогоза, тоже на своей прозрачности (требование 6).
function drawTuftDecoration(dest, destX, destY, atlas) {
  compositeRegion(dest, TUFT_BBOX.w, TUFT_BBOX.h, atlas, TUFT_BBOX.x, TUFT_BBOX.y, destX + 5, destY + 8);
}

export function buildTerrainSheet() {
  const rows = Math.ceil(TERRAIN_FRAME_COUNT / TERRAIN_COLUMNS);
  const sheet = makeCanvas(TILE * TERRAIN_COLUMNS, TILE * rows);
  const atlas = getAtlas();
  const grassA = tileFrom(atlas, ATLAS_SWATCH.grassA);
  const grassB = tileFrom(atlas, ATLAS_SWATCH.grassB);
  const path = tileFrom(atlas, ATLAS_SWATCH.path);
  const water = tileFrom(atlas, ATLAS_SWATCH.water);
  const shore = tileFrom(atlas, ATLAS_SWATCH.shore);

  function frameXY(index) {
    const col = index % TERRAIN_COLUMNS;
    const row = Math.floor(index / TERRAIN_COLUMNS);
    return { x: col * TILE, y: row * TILE };
  }

  for (let bitmask = 0; bitmask < 16; bitmask++) {
    const { x, y } = frameXY(pathTileIndex(bitmask));
    drawAutotileTile(sheet, x, y, bitmask, path, null);
  }
  for (let bitmask = 0; bitmask < 16; bitmask++) {
    const { x, y } = frameXY(waterTileIndex(bitmask));
    drawAutotileTile(sheet, x, y, bitmask, water, shore);
  }
  {
    const { x, y } = frameXY(GRASS_VARIANT_TILES[0]);
    tileFill(sheet, x, y, TILE, TILE, grassA);
  }
  {
    const { x, y } = frameXY(GRASS_VARIANT_TILES[1]);
    tileFill(sheet, x, y, TILE, TILE, grassB);
  }
  // Требование 6: мелочи поверх травы — камешек, цветок и пучок травы, один кадр на вид, на
  // прозрачном фоне.
  {
    const { x, y } = frameXY(DECORATION_TILES[0]);
    drawRockDecoration(sheet, x, y, atlas);
  }
  {
    const { x, y } = frameXY(DECORATION_TILES[1]);
    drawFlowerDecoration(sheet, x, y, atlas);
  }
  {
    const { x, y } = frameXY(DECORATION_TILES[2]);
    drawTuftDecoration(sheet, x, y, atlas);
  }
  return sheet;
}

// ---------------------------------------------------------------------------------------------
// Стены, деревья и камень: отдельные картинки объектов, каждая своего размера («Картинки»,
// требование 8 фазы). Стена — картинка на прямоугольник ЛЮБОГО размера в клетках (требование 3):
// лицевая кирпичная кладка на всю ширину/высоту прямоугольника, сверху — настоящая текстура верха
// стены из того же атласа (ревью: раньше это была заливка одним высветленным цветом).
// ---------------------------------------------------------------------------------------------

const WALL_CAP_HEIGHT = 16; // половина клетки — «парапет» над полосой препятствия

export function buildWallImage(cellsWide, cellsTall) {
  const atlas = getAtlas();
  const brick = tileFrom(atlas, BRICK_SWATCH);
  const capSwatch = tileFrom(atlas, WALL_TOP_SWATCH);

  const width = cellsWide * TILE;
  const height = cellsTall * TILE + WALL_CAP_HEIGHT;
  const image = makeCanvas(width, height);
  tileFill(image, 0, WALL_CAP_HEIGHT, width, cellsTall * TILE, brick);
  tileFill(image, 0, 0, width, WALL_CAP_HEIGHT, capSwatch);
  return image;
}

function buildTreeImage() {
  const atlas = getAtlas();
  const image = makeCanvas(TREE_BBOX.w, TREE_BBOX.h);
  compositeRegion(image, TREE_BBOX.w, TREE_BBOX.h, atlas, TREE_BBOX.x, TREE_BBOX.y, 0, 0);
  return image;
}

function buildRockImage() {
  const atlas = getAtlas();
  const image = makeCanvas(ROCK_BBOX.w, ROCK_BBOX.h);
  compositeRegion(image, ROCK_BBOX.w, ROCK_BBOX.h, atlas, ROCK_BBOX.x, ROCK_BBOX.y, 0, 0);
  return image;
}

// Требование 14: дерево — footprint 0,8 × 0,5 у основания ствола, камень — по своему основанию.
export const TREE_FOOTPRINT_SIZE = [0.8, 0.5];
export const ROCK_FOOTPRINT_SIZE = [0.8, 0.6];
export const TREE_IMAGE_SIZE = [TREE_BBOX.w / TILE, TREE_BBOX.h / TILE];
export const ROCK_IMAGE_SIZE = [ROCK_BBOX.w / TILE, ROCK_BBOX.h / TILE];

// ---------------------------------------------------------------------------------------------
// Размеры стен, нужные текущей карте — читаем `location.txt` тем же слиянием в прямоугольники,
// что и сборщик сцены (`rpgLocationMap.mjs`), чтобы не рисовать размеры, которых нет, и не
// разойтись со сценой (требование 3: «стены сливаются в прямоугольники»).
// ---------------------------------------------------------------------------------------------

export function wallImageManifest(locationText) {
  const map = parseLocationMap(locationText);
  const rects = extractRectangles(map, CELL.WALL);
  const sizes = new Map();
  for (const rect of rects) sizes.set(`${rect.width}x${rect.height}`, { width: rect.width, height: rect.height });
  return [...sizes.values()].sort((a, b) => a.width - b.width || a.height - b.height);
}

export function wallImageName(width, height) {
  return `wall_${width}x${height}`;
}

// Ревью: старые карты оставляли в `games/rpg/images/` файлы `wall_*.png`, которых текущая карта
// уже не объявляет в `game.json` (например, размеров прежнего лабиринта) — они никуда не делись
// сами при смене карты, просто переставали упоминаться. Чистая функция: какие уже лежащие на
// диске `wall_*.png` не входят в нынешний манифест и должны быть удалены перед записью новых.
export function staleWallImageFiles(existingFileNames, wallSizes) {
  const wanted = new Set(wallSizes.map(({ width, height }) => `${wallImageName(width, height)}.png`));
  return existingFileNames.filter((name) => /^wall_.*\.png$/.test(name) && !wanted.has(name));
}

// `files.images` game.json — требование 8: герой/гоблин/орк с `frames`/`columns`/`size`/`anchor`/
// `offset`; листы удара — `frames: 24, columns: 6`; набор плиток без `frame_time`/`frame_by`;
// стены/дерево/камень — своим размером.
export function buildImagesDeclaration(wallSizes) {
  const images = {
    hero: { path: "images/hero.png", frames: 45, columns: 9, frame_by: "frame", size: CHARACTER_IMAGE_SIZE, anchor: "bottom", offset: CHARACTER_IMAGE_OFFSET },
    goblin: { path: "images/goblin.png", frames: 45, columns: 9, frame_by: "frame", size: CHARACTER_IMAGE_SIZE, anchor: "bottom", offset: CHARACTER_IMAGE_OFFSET },
    orc: { path: "images/orc.png", frames: 45, columns: 9, frame_by: "frame", size: CHARACTER_IMAGE_SIZE, anchor: "bottom", offset: CHARACTER_IMAGE_OFFSET },
    hero_attack: { path: "images/hero_attack.png", frames: 24, columns: 6, frame_by: "frame", size: attackImageSize(CHARACTERS.hero.attackFrame), anchor: "bottom", offset: attackImageOffset(CHARACTERS.hero.attackFrame) },
    goblin_attack: { path: "images/goblin_attack.png", frames: 24, columns: 6, frame_by: "frame", size: attackImageSize(CHARACTERS.goblin.attackFrame), anchor: "bottom", offset: attackImageOffset(CHARACTERS.goblin.attackFrame) },
    orc_attack: { path: "images/orc_attack.png", frames: 24, columns: 6, frame_by: "frame", size: attackImageSize(CHARACTERS.orc.attackFrame), anchor: "bottom", offset: attackImageOffset(CHARACTERS.orc.attackFrame) },
    terrain: { path: "images/terrain.png", frames: TERRAIN_FRAME_COUNT, columns: TERRAIN_COLUMNS },
    tree: { path: "images/tree.png", size: TREE_IMAGE_SIZE, anchor: "bottom" },
    rock: { path: "images/rock.png", size: ROCK_IMAGE_SIZE, anchor: "bottom" },
    marker: { path: "images/marker.png" },
  };
  for (const { width, height } of wallSizes) {
    const name = wallImageName(width, height);
    images[name] = { path: `images/${name}.png`, size: [width, height + WALL_CAP_HEIGHT / TILE], anchor: "bottom" };
  }
  return images;
}

// ---------------------------------------------------------------------------------------------
// Файл авторов — «Картинки и авторы», пункт 3: каждая использованная часть, таблица цветов и кусок
// атласа перечислены по имени, с авторами, лицензиями и ссылкой на страницу источника (лицензии
// CC-BY-SA/OGA-BY этого требуют; в игру `Attribution.txt` не попадает, поэтому CREDITS.txt не
// отсылает к нему, а называет авторов сам). Источник строк — `sources.json`.
// ---------------------------------------------------------------------------------------------

function buildCreditsText(sources) {
  const lines = ["Авторы графики ролевой игры (LPC)", ""];
  for (const entry of sources.parts) {
    lines.push(entry.usedAs);
    lines.push(`  Источник: ${entry.sourcePath} (коммит ${sources.generator.commit}, ${sources.generator.repo})`);
    lines.push(`  Авторы: ${entry.authors.join(", ")}`);
    lines.push(`  Лицензии: ${entry.licenses.join(", ")}`);
    lines.push(`  Страницы: ${entry.pages.join(", ")}`);
    lines.push("");
  }
  for (const entry of sources.palettes) {
    lines.push(entry.usedAs);
    lines.push(`  Источник: ${entry.sourcePath} (коммит ${sources.generator.commit}, ${sources.generator.repo})`);
    lines.push(`  Авторы: ${entry.authors.join(", ")}`);
    lines.push(`  Лицензии: ${entry.licenses.join(", ")}`);
    lines.push("");
  }
  for (const entry of sources.atlasPieces) {
    lines.push(entry.usedAs);
    lines.push(`  Источник: ${sources.atlas.page}`);
    // Ревью: если по атласу автора/лицензию установить не удалось, так и пишем — не приписываем
    // наугад (пустой список в `sources.json`, не выдуманное имя).
    lines.push(`  Авторы: ${entry.authors.length > 0 ? entry.authors.join(", ") : "не установлены по атласу"}`);
    lines.push(`  Лицензии: ${entry.licenses.length > 0 ? entry.licenses.join(", ") : "не установлены по атласу"}`);
    lines.push("");
  }
  return lines.join("\n");
}

// ---------------------------------------------------------------------------------------------
// Точка входа: собирает все картинки и CREDITS.txt из исходников, без сети.
// ---------------------------------------------------------------------------------------------

const isMainModule = import.meta.url === pathToFileURL(process.argv[1] ?? "").href;
if (isMainModule) {
  mkdirSync(imagesDir, { recursive: true });

  for (const [name, character] of Object.entries(CHARACTERS)) {
    const sheet = buildMainSheet(character.layers);
    writePngFile(resolve(imagesDir, `${name}.png`), sheet);
    console.log(`${name}.png: ${sheet.width}×${sheet.height}`);

    const attackSheet = buildAttackSheet(character.attackFrame, character.layers);
    writePngFile(resolve(imagesDir, `${name}_attack.png`), attackSheet);
    console.log(`${name}_attack.png: ${attackSheet.width}×${attackSheet.height}`);
  }

  const terrain = buildTerrainSheet();
  writePngFile(resolve(imagesDir, "terrain.png"), terrain);
  console.log(`terrain.png: ${terrain.width}×${terrain.height}`);

  writePngFile(resolve(imagesDir, "tree.png"), buildTreeImage());
  writePngFile(resolve(imagesDir, "rock.png"), buildRockImage());
  console.log("tree.png, rock.png готовы");

  const locationText = readFileSync(resolve(lpcDir, "location.txt"), "utf8");
  const wallSizes = wallImageManifest(locationText);
  const stale = staleWallImageFiles(readdirSync(imagesDir), wallSizes);
  for (const name of stale) unlinkSync(resolve(imagesDir, name));
  if (stale.length > 0) console.log(`удалены устаревшие картинки стен: ${stale.join(", ")}`);
  for (const { width, height } of wallSizes) {
    const image = buildWallImage(width, height);
    writePngFile(resolve(imagesDir, `${wallImageName(width, height)}.png`), image);
  }
  console.log(`стены: ${wallSizes.length} размеров прямоугольников`);

  const sources = JSON.parse(readFileSync(resolve(lpcDir, "sources.json"), "utf8"));
  writeFileSync(resolve(gameDir, "CREDITS.txt"), buildCreditsText(sources));
  console.log("CREDITS.txt готов");

  // Требование 9: сцена по размеру карты, view_height 12, y_sort, фон — цвет травы (образец
  // атласа); требование 8: объявления картинок. Размер сцены берётся из `location.txt`, а не
  // задан в скрипте числами (карта — источник истины и для стен, и для габаритов сцены).
  const map = parseLocationMap(locationText);
  const gameJsonPath = resolve(gameDir, "game.json");
  const gameJson = JSON.parse(readFileSync(gameJsonPath, "utf8"));
  gameJson.scene = { width: map.width, height: map.height, background: "#2f8534", view_height: 12, y_sort: true };
  gameJson.files.images = buildImagesDeclaration(wallSizes);
  writeFileSync(gameJsonPath, JSON.stringify(gameJson, null, 2) + "\n");
  console.log("game.json обновлён (scene, files.images)");
}
