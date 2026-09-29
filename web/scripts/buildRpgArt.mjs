// Сборка графики ролевой игры из исходников LPC («Фаза-02-враг-на-пути», пункты 1–8; местность LPC
// этого файла с «Фазы-02-5-первая-нарисованная-локация» ушла в web/scripts/buildRpgVillageScene.mjs
// — требование 17 той фазы): листы героя, гоблина и орка из скачанных частей генератора персонажей
// (основной лист 9×5 и отдельный лист удара). Сеть скрипту не нужна — все исходники уже лежат в
// `rpg-lpc/`. Запуск: `node scripts/buildRpgArt.mjs` из `web/`. Результат детерминирован: повторный
// запуск даёт те же байты (пункт 2).

import { PNG } from "pngjs";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { hexToRgb } from "./generateDemoImages.mjs";

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

// `files.images` game.json — требование 8 фазы «Фаза-02-враг-на-пути»: герой/гоблин/орк с
// `frames`/`columns`/`size`/`anchor`/`offset`; листы удара — `frames: 24, columns: 6`. Местность
// (`terrain`, стены, дерево, камень) сюда больше не входит — её объявления собирает
// `buildRpgVillageScene.mjs` из уже нарисованных кусков деревни.
export function buildImagesDeclaration() {
  return {
    hero: { path: "images/hero.png", frames: 45, columns: 9, frame_by: "frame", size: CHARACTER_IMAGE_SIZE, anchor: "bottom", offset: CHARACTER_IMAGE_OFFSET },
    goblin: { path: "images/goblin.png", frames: 45, columns: 9, frame_by: "frame", size: CHARACTER_IMAGE_SIZE, anchor: "bottom", offset: CHARACTER_IMAGE_OFFSET },
    orc: { path: "images/orc.png", frames: 45, columns: 9, frame_by: "frame", size: CHARACTER_IMAGE_SIZE, anchor: "bottom", offset: CHARACTER_IMAGE_OFFSET },
    hero_attack: { path: "images/hero_attack.png", frames: 24, columns: 6, frame_by: "frame", size: attackImageSize(CHARACTERS.hero.attackFrame), anchor: "bottom", offset: attackImageOffset(CHARACTERS.hero.attackFrame) },
    goblin_attack: { path: "images/goblin_attack.png", frames: 24, columns: 6, frame_by: "frame", size: attackImageSize(CHARACTERS.goblin.attackFrame), anchor: "bottom", offset: attackImageOffset(CHARACTERS.goblin.attackFrame) },
    orc_attack: { path: "images/orc_attack.png", frames: 24, columns: 6, frame_by: "frame", size: attackImageSize(CHARACTERS.orc.attackFrame), anchor: "bottom", offset: attackImageOffset(CHARACTERS.orc.attackFrame) },
    marker: { path: "images/marker.png" },
  };
}

// ---------------------------------------------------------------------------------------------
// Файл авторов — «Картинки и авторы», пункт 3: каждая использованная часть и таблица цветов
// перечислены по имени, с авторами, лицензиями и ссылкой на страницу источника (лицензии
// CC-BY-SA/OGA-BY этого требуют; в игру `Attribution.txt` не попадает, поэтому CREDITS.txt не
// отсылает к нему, а называет авторов сам). Источник строк — `sources.json`. Экспортирована:
// `buildRpgVillageScene.mjs` дописывает следом авторство нарисованных кусков деревни и сам пишет
// готовый CREDITS.txt («Фаза-02-5-первая-нарисованная-локация», требование 17 — местность и её
// программы уходят из этого файла, а не только из сцены).
export function buildCreditsText(sources) {
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
  return lines.join("\n");
}

// ---------------------------------------------------------------------------------------------
// Точка входа: собирает листы персонажей и их часть `game.json`/CREDITS.txt из исходников, без
// сети. Сцену, размер сцены и картинки деревни собирает отдельно `buildRpgVillageScene.mjs` —
// оба скрипта сливают свои ключи `files.images` в общий `game.json`, не затирая друг друга.
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

  const sources = JSON.parse(readFileSync(resolve(lpcDir, "sources.json"), "utf8"));
  writeFileSync(resolve(gameDir, "CREDITS.txt"), buildCreditsText(sources));
  console.log("CREDITS.txt готов (авторы персонажей; куски деревни дописывает buildRpgVillageScene.mjs)");

  const gameJsonPath = resolve(gameDir, "game.json");
  const gameJson = JSON.parse(readFileSync(gameJsonPath, "utf8"));
  gameJson.files.images = { ...gameJson.files.images, ...buildImagesDeclaration() };
  writeFileSync(gameJsonPath, JSON.stringify(gameJson, null, 2) + "\n");
  console.log("game.json обновлён (files.images: герой, гоблин, орк, отметка)");
}
