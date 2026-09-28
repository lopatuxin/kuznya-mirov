import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";
import {
  compositeRegion,
  hexToRgb,
  recolorSkin,
  insideAutotileBlob,
  wallImageManifest,
  wallImageName,
  staleWallImageFiles,
  buildImagesDeclaration,
  buildMainSheet,
  buildAttackSheet,
  buildTerrainSheet,
  buildWallImage,
  attackImageSize,
  attackImageOffset,
  CHARACTERS,
  CHARACTER_IMAGE_SIZE,
  CHARACTER_IMAGE_OFFSET,
  TREE_IMAGE_SIZE,
  ROCK_IMAGE_SIZE,
  TREE_FOOTPRINT_SIZE,
  ROCK_FOOTPRINT_SIZE,
} from "./buildRpgArt.mjs";
import { TERRAIN_COLUMNS, TERRAIN_FRAME_COUNT, GRASS_VARIANT_TILES, DECORATION_TILES, waterTileIndex } from "./rpgTerrainTiles.mjs";

const scriptDir = fileURLToPath(new URL(".", import.meta.url));
const gameDir = resolve(scriptDir, "../../games/rpg");
const lpcDir = resolve(scriptDir, "rpg-lpc");

function makePng(width, height) {
  const png = new PNG({ width, height });
  png.data.fill(0);
  return png;
}

function setPixel(png, x, y, r, g, b, a) {
  const i = (y * png.width + x) * 4;
  png.data[i] = r;
  png.data[i + 1] = g;
  png.data[i + 2] = b;
  png.data[i + 3] = a;
}

function getPixel(png, x, y) {
  const i = (y * png.width + x) * 4;
  return [png.data[i], png.data[i + 1], png.data[i + 2], png.data[i + 3]];
}

function hasOpaquePixel(png, x0, y0, w, h) {
  for (let y = y0; y < y0 + h; y++) {
    for (let x = x0; x < x0 + w; x++) {
      if (getPixel(png, x, y)[3] > 0) return true;
    }
  }
  return false;
}

describe("compositeRegion", () => {
  it("непрозрачный источник полностью заменяет точку назначения", () => {
    const dest = makePng(2, 2);
    setPixel(dest, 0, 0, 10, 10, 10, 255);
    const src = makePng(1, 1);
    setPixel(src, 0, 0, 200, 100, 50, 255);
    compositeRegion(dest, 1, 1, src, 0, 0, 0, 0);
    expect(getPixel(dest, 0, 0)).toEqual([200, 100, 50, 255]);
  });

  it("полностью прозрачный источник не трогает назначение", () => {
    const dest = makePng(1, 1);
    setPixel(dest, 0, 0, 10, 20, 30, 255);
    const src = makePng(1, 1); // альфа 0 по умолчанию
    compositeRegion(dest, 1, 1, src, 0, 0, 0, 0);
    expect(getPixel(dest, 0, 0)).toEqual([10, 20, 30, 255]);
  });

  it("полупрозрачный источник смешивается с назначением", () => {
    const dest = makePng(1, 1);
    setPixel(dest, 0, 0, 0, 0, 0, 255);
    const src = makePng(1, 1);
    setPixel(src, 0, 0, 255, 255, 255, 128);
    compositeRegion(dest, 1, 1, src, 0, 0, 0, 0);
    const [r, g, b, a] = getPixel(dest, 0, 0);
    expect(r).toBeGreaterThan(100);
    expect(r).toBeLessThan(255);
    expect(a).toBe(255);
  });

  it("копирует область по смещению назначения и источника", () => {
    const dest = makePng(3, 1);
    const src = makePng(3, 1);
    setPixel(src, 1, 0, 9, 9, 9, 255);
    compositeRegion(dest, 1, 1, src, 1, 0, 2, 0);
    expect(getPixel(dest, 2, 0)).toEqual([9, 9, 9, 255]);
    expect(getPixel(dest, 0, 0)[3]).toBe(0);
  });
});

describe("hexToRgb (переиспользован из generateDemoImages.mjs)", () => {
  it("разбирает цвет по каналам", () => {
    expect(hexToRgb("#271920")).toEqual([0x27, 0x19, 0x20]);
  });
});

describe("recolorSkin", () => {
  it("заменяет цвет из таблицы source в соответствующий цвет target, с допуском", () => {
    const png = makePng(2, 1);
    setPixel(png, 0, 0, 0xcc, 0x86, 0x65, 255); // точное совпадение с source[3]
    setPixel(png, 1, 0, 0xcb, 0x87, 0x64, 255); // совпадение с допуском ±1
    recolorSkin(png, ["#271920", "#271920", "#99423c", "#cc8665"], ["#000000", "#000000", "#000000", "#19541d"]);
    expect(getPixel(png, 0, 0).slice(0, 3)).toEqual([0x19, 0x54, 0x1d]);
    expect(getPixel(png, 1, 0).slice(0, 3)).toEqual([0x19, 0x54, 0x1d]);
  });

  it("не трогает цвет, которого нет в таблице source", () => {
    const png = makePng(1, 1);
    setPixel(png, 0, 0, 1, 2, 3, 255);
    recolorSkin(png, ["#271920"], ["#ffffff"]);
    expect(getPixel(png, 0, 0)).toEqual([1, 2, 3, 255]);
  });

  it("не трогает прозрачные точки", () => {
    const png = makePng(1, 1);
    setPixel(png, 0, 0, 0x27, 0x19, 0x20, 0);
    recolorSkin(png, ["#271920"], ["#ffffff"]);
    expect(getPixel(png, 0, 0)).toEqual([0x27, 0x19, 0x20, 0]);
  });
});

describe("insideAutotileBlob", () => {
  const FULL = { top: 0, right: 0, bottom: 0, left: 0 };
  const ISOLATED = { top: 8, right: 8, bottom: 8, left: 8 };

  it("без отступов (маска 15 — все соседи те же) — вся плитка внутри пятна", () => {
    expect(insideAutotileBlob(0, 0, FULL)).toBe(true);
    expect(insideAutotileBlob(31, 31, FULL)).toBe(true);
    expect(insideAutotileBlob(16, 16, FULL)).toBe(true);
  });

  it("изолированная плитка (маска 0) — угол среза, центр внутри", () => {
    expect(insideAutotileBlob(16, 16, ISOLATED)).toBe(true);
    expect(insideAutotileBlob(0, 0, ISOLATED)).toBe(false); // угол тайла — точно за пределами отступа
  });

  it("связанная сторона без отступа — до самого края плитки", () => {
    const topOnly = { top: 0, right: 8, bottom: 8, left: 8 };
    expect(insideAutotileBlob(16, 0, topOnly)).toBe(true); // верхний край, N — связан
  });
});

describe("wallImageManifest / wallImageName", () => {
  it("собирает размеры прямоугольников стен из карты, включая блок толще одной клетки", () => {
    // Верхняя стена — горизонтальная полоса 5×1, блок 2×2 слева, отдельная вертикальная 1×3.
    const text = "#####\n##...\n##...\n#....\n#....\n";
    const manifest = wallImageManifest(text);
    expect(manifest).toEqual(
      expect.arrayContaining([
        { width: 5, height: 1 },
        { width: 2, height: 2 },
      ]),
    );
  });

  it("имя картинки зависит от ширины и высоты", () => {
    expect(wallImageName(5, 1)).toBe("wall_5x1");
    expect(wallImageName(2, 3)).toBe("wall_2x3");
  });
});

describe("staleWallImageFiles — ревью: остатки прежней карты в images/", () => {
  const wallSizes = [{ width: 5, height: 1 }, { width: 2, height: 2 }];

  it("находит wall_*.png на диске, которых нет в текущем манифесте", () => {
    const existing = ["wall_5x1.png", "wall_2x2.png", "wall_17x2.png", "wall_1x1.png", "hero.png"];
    expect(staleWallImageFiles(existing, wallSizes)).toEqual(["wall_17x2.png", "wall_1x1.png"]);
  });

  it("не трогает файлы из манифеста и не-стеновые файлы", () => {
    const existing = ["wall_5x1.png", "wall_2x2.png", "terrain.png", "goblin_attack.png"];
    expect(staleWallImageFiles(existing, wallSizes)).toEqual([]);
  });
});

describe("buildImagesDeclaration", () => {
  const images = buildImagesDeclaration([{ width: 4, height: 1 }, { width: 2, height: 3 }]);

  it("герой, гоблин и орк — основной лист 45 кадров сеткой 9, frame_by и якорь снизу", () => {
    for (const name of ["hero", "goblin", "orc"]) {
      expect(images[name]).toMatchObject({
        frames: 45,
        columns: 9,
        frame_by: "frame",
        size: CHARACTER_IMAGE_SIZE,
        anchor: "bottom",
        offset: CHARACTER_IMAGE_OFFSET,
      });
    }
  });

  it("листы удара — 24 кадра сеткой 6, свой размер и сдвиг по стороне кадра", () => {
    for (const [name, character] of [["hero_attack", "hero"], ["goblin_attack", "goblin"], ["orc_attack", "orc"]]) {
      expect(images[name]).toMatchObject({
        frames: 24,
        columns: 6,
        frame_by: "frame",
        size: attackImageSize(CHARACTERS[character].attackFrame),
        anchor: "bottom",
        offset: attackImageOffset(CHARACTERS[character].attackFrame),
      });
    }
  });

  it("набор плиток земли — без frame_time и frame_by", () => {
    expect(images.terrain).toEqual({ path: "images/terrain.png", frames: TERRAIN_FRAME_COUNT, columns: TERRAIN_COLUMNS });
  });

  it("дерево и камень — своим размером, привязка снизу", () => {
    expect(images.tree).toEqual({ path: "images/tree.png", size: TREE_IMAGE_SIZE, anchor: "bottom" });
    expect(images.rock).toEqual({ path: "images/rock.png", size: ROCK_IMAGE_SIZE, anchor: "bottom" });
  });

  it("стены объявлены под каждый нужный размер прямоугольника", () => {
    expect(images.wall_4x1.size).toEqual([4, 1.5]);
    expect(images.wall_2x3.size).toEqual([2, 3.5]);
  });

  it("отметка щелчка остаётся из демо-генератора", () => {
    expect(images.marker).toEqual({ path: "images/marker.png" });
  });
});

// Договор с данными игры («Фаза-02-враг-на-пути», требования 5, 6, 14) — сборка идёт из
// реальных исходников `rpg-lpc/`, повторный запуск даёт те же байты.
describe("buildMainSheet — договор с данными игры", () => {
  it("основной лист любого персонажа — 576×320 (9×5 кадров 64×64)", () => {
    for (const character of Object.values(CHARACTERS)) {
      const sheet = buildMainSheet(character.layers);
      expect([sheet.width, sheet.height]).toEqual([576, 320]);
    }
  });

  it("повторная сборка даёт те же байты", () => {
    const a = buildMainSheet(CHARACTERS.goblin.layers);
    const b = buildMainSheet(CHARACTERS.goblin.layers);
    expect(a.data.equals(b.data)).toBe(true);
  });

  it("непрозрачность основного листа — во всех 9 кадрах ходьбы каждой стороны и в 6 кадрах падения, у всех трёх персонажей", () => {
    const FRAME = 64;
    for (const character of Object.values(CHARACTERS)) {
      const sheet = buildMainSheet(character.layers);
      for (let row = 0; row < 4; row++) {
        for (let col = 0; col < 9; col++) {
          expect(hasOpaquePixel(sheet, col * FRAME, row * FRAME, FRAME, FRAME)).toBe(true);
        }
      }
      for (let col = 0; col < 6; col++) {
        expect(hasOpaquePixel(sheet, col * FRAME, 4 * FRAME, FRAME, FRAME)).toBe(true);
      }
      // Столбцы 6–8 строки падения не объявлены («падение, 6 кадров») и остаются пустыми.
      for (let col = 6; col < 9; col++) {
        expect(hasOpaquePixel(sheet, col * FRAME, 4 * FRAME, FRAME, FRAME)).toBe(false);
      }
    }
  });

  it("кожа гоблина и орка зелёная в большинстве точек кадра стойки (вниз)", () => {
    const FRAME = 64;
    for (const name of ["goblin", "orc"]) {
      const sheet = buildMainSheet(CHARACTERS[name].layers);
      let opaque = 0;
      let greenDominant = 0;
      for (let y = 2 * FRAME; y < 3 * FRAME; y++) {
        for (let x = 0; x < FRAME; x++) {
          const [r, g, b, a] = getPixel(sheet, x, y);
          if (a < 255) continue;
          opaque++;
          if (g > r && g > b) greenDominant++;
        }
      }
      expect(opaque).toBeGreaterThan(0);
      expect(greenDominant / opaque).toBeGreaterThan(0.4);
    }
  });
});

describe("buildAttackSheet — договор с данными игры (оружие видно на взмахе)", () => {
  it("размер листа удара — 6×4 кадра стороны character.attackFrame", () => {
    for (const character of Object.values(CHARACTERS)) {
      const sheet = buildAttackSheet(character.attackFrame, character.layers);
      expect([sheet.width, sheet.height]).toEqual([character.attackFrame * 6, character.attackFrame * 4]);
    }
  });

  it("повторная сборка даёт те же байты", () => {
    const a = buildAttackSheet(CHARACTERS.hero.attackFrame, CHARACTERS.hero.layers);
    const b = buildAttackSheet(CHARACTERS.hero.attackFrame, CHARACTERS.hero.layers);
    expect(a.data.equals(b.data)).toBe(true);
  });

  it("в каждом из 24 кадров любого персонажа видно тело (не только оружие)", () => {
    for (const character of Object.values(CHARACTERS)) {
      const sheet = buildAttackSheet(character.attackFrame, character.layers);
      const frame = character.attackFrame;
      for (let row = 0; row < 4; row++) {
        for (let col = 0; col < 6; col++) {
          expect(hasOpaquePixel(sheet, col * frame, row * frame, frame, frame)).toBe(true);
        }
      }
    }
  });

  // Кадры, где источник LPC-генератора сам не рисует оружие в этом взмахе — проверено напрямую по
  // файлу, это не огрех сборки: `weapon/dagger/slash.png` целиком пустой в строке 0 (направление
  // «вверх», лицом от камеры — маленький кинжал в этом ракурсе у автора не виден весь замах), а
  // `weapon/mace/behind_attack.png` в кадре (строка 0, столбец 1) целиком перекрыт телом (кадр
  // раннего замаха, булава ещё не вышла из-за спины) при пустом переднем слое в этом же кадре.
  const NO_WEAPON_PIXEL_EXCEPTIONS = {
    goblin: new Set([0, 1, 2, 3, 4, 5]), // вся строка 0 (вверх)
    orc: new Set([1]), // строка 0, столбец 1
    hero: new Set(),
  };

  it("в каждом из 24 кадров видно именно оружие, а не только тело, кроме документированных пробелов исходника", () => {
    // Точка «тела» — недостаточное доказательство: до правки требования 4 меч героя и топор орка
    // пропадали именно на взмахе, а тело оставалось видно. Сравниваем с листом, собранным без
    // оружейного слоя вовсе — точка, непрозрачная в полном листе и прозрачная в безоружном,
    // однозначно принадлежит оружию.
    for (const [name, character] of Object.entries(CHARACTERS)) {
      const withWeapon = buildAttackSheet(character.attackFrame, character.layers);
      const bodyOnlyLayers = character.layers.filter((layer) => !layer.dir.startsWith("weapon/"));
      const withoutWeapon = buildAttackSheet(character.attackFrame, bodyOnlyLayers);
      const frame = character.attackFrame;
      const exceptions = NO_WEAPON_PIXEL_EXCEPTIONS[name];
      for (let row = 0; row < 4; row++) {
        for (let col = 0; col < 6; col++) {
          // Не только «непрозрачно в одном, прозрачно в другом» — оружие, нарисованное поверх уже
          // непрозрачного тела (тем же слоем с более высоким zPos), тоже меняет цвет точки.
          let weaponPixelFound = false;
          for (let y = row * frame; y < (row + 1) * frame && !weaponPixelFound; y++) {
            for (let x = col * frame; x < (col + 1) * frame; x++) {
              const withPixel = getPixel(withWeapon, x, y);
              const withoutPixel = getPixel(withoutWeapon, x, y);
              if (withPixel.some((channel, index) => channel !== withoutPixel[index])) {
                weaponPixelFound = true;
                break;
              }
            }
          }
          const isKnownGap = row === 0 && exceptions.has(col); // все исключения сейчас лежат в строке 0 (вверх)
          expect(weaponPixelFound).toBe(!isKnownGap);
        }
      }
    }
  });

  it("ступни тела в кадре удара стоят там же, что и в основном листе (нижняя опаковая строка на том же расстоянии от низа кадра)", () => {
    // Требование 5: тело в кадре удара стоит так же относительно ступней, что и в основном листе.
    // Проверяем на кадре 0 строки «вниз» (row=2): нижняя непрозрачная строка пикселей тела должна
    // быть на одном и том же смещении от низа кадра что в main (64px), что в attack (frameSize px),
    // с точностью до целого пикселя центрирования.
    for (const character of Object.values(CHARACTERS)) {
      const FRAME = 64;
      const main = buildMainSheet(character.layers);
      const attack = buildAttackSheet(character.attackFrame, character.layers);
      function lastOpaqueRowFromBottom(png, x0, y0, size) {
        for (let dy = size - 1; dy >= 0; dy--) {
          for (let dx = 0; dx < size; dx++) {
            if (getPixel(png, x0 + dx, y0 + dy)[3] > 0) return size - 1 - dy;
          }
        }
        return -1;
      }
      const mainGap = lastOpaqueRowFromBottom(main, 0, 2 * FRAME, FRAME);
      const attackFrame = character.attackFrame;
      const padding = (attackFrame - FRAME) / 2;
      const attackGap = lastOpaqueRowFromBottom(attack, 0, 2 * attackFrame, attackFrame);
      expect(mainGap).toBeGreaterThanOrEqual(0);
      expect(attackGap).toBeCloseTo(mainGap + padding, 0);
    }
  });
});

describe("buildTerrainSheet — договор с данными игры", () => {
  it("TERRAIN_FRAME_COUNT кадров сеткой TERRAIN_COLUMNS", () => {
    const sheet = buildTerrainSheet();
    expect([sheet.width, sheet.height]).toEqual([TERRAIN_COLUMNS * 32, Math.ceil(TERRAIN_FRAME_COUNT / TERRAIN_COLUMNS) * 32]);
  });

  it("повторная сборка даёт те же байты", () => {
    const a = buildTerrainSheet();
    const b = buildTerrainSheet();
    expect(a.data.equals(b.data)).toBe(true);
  });

  function framePixels(sheet, index) {
    const TILE = 32;
    const col = index % TERRAIN_COLUMNS;
    const row = Math.floor(index / TERRAIN_COLUMNS);
    const pixels = [];
    for (let y = 0; y < TILE; y++) {
      const rowPixels = [];
      for (let x = 0; x < TILE; x++) rowPixels.push(getPixel(sheet, col * TILE + x, row * TILE + y));
      pixels.push(rowPixels);
    }
    return pixels;
  }

  it("оба варианта травы (требование 6) — текстурная трава, бесшовная через 32 точки, разный рисунок, близкий тон", () => {
    const sheet = buildTerrainSheet();
    const TILE = 32;
    const averages = [];
    const allPixels = [];
    for (const index of GRASS_VARIANT_TILES) {
      const pixels = framePixels(sheet, index);
      allPixels.push(pixels);
      let opaque = 0;
      let greenDominant = 0;
      let sr = 0, sg = 0, sb = 0;
      for (const row of pixels) {
        for (const [r, g, b, a] of row) {
          if (a === 0) continue;
          opaque++;
          sr += r; sg += g; sb += b;
          if (g > r + 15 && g > b + 15) greenDominant++;
        }
      }
      expect(opaque).toBe(TILE * TILE); // базовый слой не должен просвечивать
      expect(greenDominant / opaque).toBeGreaterThan(0.9);
      averages.push([sr / opaque, sg / opaque, sb / opaque]);

      // Бесшовность при повторе через 32 точки: левый край должен совпадать с правым (в этом же
      // кадре, соседняя копия начинается с той же точки), верхний — с нижним.
      let seamDiff = 0, seamCount = 0;
      for (let y = 0; y < TILE; y++) {
        const [r0, g0, b0] = pixels[y][0];
        const [r1, g1, b1] = pixels[y][TILE - 1];
        seamDiff += Math.abs(r0 - r1) + Math.abs(g0 - g1) + Math.abs(b0 - b1);
        seamCount++;
      }
      for (let x = 0; x < TILE; x++) {
        const [r0, g0, b0] = pixels[0][x];
        const [r1, g1, b1] = pixels[TILE - 1][x];
        seamDiff += Math.abs(r0 - r1) + Math.abs(g0 - g1) + Math.abs(b0 - b1);
        seamCount++;
      }
      // Порог заметно выше нуля: мелкая шумовая текстура естественно не даёт идеального
      // побитового совпадения на стыке, но заметный скачок (проверенные плохие вырезки давали
      // 100+) отличим от него с запасом.
      expect(seamDiff / seamCount).toBeLessThan(30);
    }
    // Ревью: варианты должны заметно различаться рисунком, но быть близки по среднему тону —
    // иначе смена варианта либо незаметна, либо читается пятном другого цвета.
    const [a, b] = averages;
    const toneDistance = Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    expect(toneDistance).toBeLessThan(15);

    let differingPixels = 0;
    for (let y = 0; y < TILE; y++) {
      for (let x = 0; x < TILE; x++) {
        const [r0, g0, b0] = allPixels[0][y][x];
        const [r1, g1, b1] = allPixels[1][y][x];
        if (Math.abs(r0 - r1) > 8 || Math.abs(g0 - g1) > 8 || Math.abs(b0 - b1) > 8) differingPixels++;
      }
    }
    expect(differingPixels / (TILE * TILE)).toBeGreaterThan(0.2);
  });

  it("вода — бесшовна через 32 точки (ревью: шов между соседними клетками пруда)", () => {
    // Кадр с маской 15 (все соседи — тоже вода) — сплошная вода без берега по краям, ровно то,
    // что рисуется между двумя соседними клетками воды внутри пруда.
    const sheet = buildTerrainSheet();
    const pixels = framePixels(sheet, waterTileIndex(15));
    let opaque = 0;
    for (const row of pixels) for (const [, , , a] of row) if (a > 0) opaque++;
    expect(opaque).toBe(32 * 32); // сплошная вода, без прозрачных прорех
    let seamDiff = 0, seamCount = 0;
    for (let y = 0; y < 32; y++) {
      const [r0, g0, b0] = pixels[y][0];
      const [r1, g1, b1] = pixels[y][31];
      seamDiff += Math.abs(r0 - r1) + Math.abs(g0 - g1) + Math.abs(b0 - b1);
      seamCount++;
    }
    for (let x = 0; x < 32; x++) {
      const [r0, g0, b0] = pixels[0][x];
      const [r1, g1, b1] = pixels[31][x];
      seamDiff += Math.abs(r0 - r1) + Math.abs(g0 - g1) + Math.abs(b0 - b1);
      seamCount++;
    }
    expect(seamDiff / seamCount).toBeLessThan(30);
  });

  it("верх стены — настоящая текстура из атласа, бесшовная через 32 точки (не заливка цветом)", () => {
    const image = buildWallImage(1, 1);
    // Кадр верха — первые WALL_CAP_HEIGHT строк изображения.
    const WALL_CAP_HEIGHT = 16;
    let seamDiff = 0, seamCount = 0;
    for (let y = 0; y < WALL_CAP_HEIGHT; y++) {
      const [r0, g0, b0] = getPixel(image, 0, y);
      const [r1, g1, b1] = getPixel(image, 31, y);
      seamDiff += Math.abs(r0 - r1) + Math.abs(g0 - g1) + Math.abs(b0 - b1);
      seamCount++;
    }
    expect(seamDiff / seamCount).toBeLessThan(10);
    // Не заливка одним цветом — настоящая текстура (требование 2, ревью).
    const colors = new Set();
    for (let y = 0; y < WALL_CAP_HEIGHT; y++) for (let x = 0; x < 32; x++) colors.add(getPixel(image, x, y).slice(0, 3).join(","));
    expect(colors.size).toBeGreaterThan(1);
  });

  it("требование 6: мелочи — камешек, цветок и пучок травы, по одному кадру на вид, на прозрачном фоне", () => {
    // Один кадр на вид (не пара под каждый вариант травы, ревью): DECORATION_TILES — 3 записи.
    // Кадр не залит травой — за пределами самой мелочи он прозрачен, трава клетки видна сквозь
    // него из слоя под ним.
    expect(DECORATION_TILES).toHaveLength(3);
    const sheet = buildTerrainSheet();
    const TILE = 32;
    function frameStats(index, predicate) {
      const pixels = framePixels(sheet, index);
      let opaque = 0;
      let matching = false;
      let backgroundColorHit = false;
      for (const row of pixels) {
        for (const [r, g, b, a] of row) {
          if (a === 0) continue;
          opaque++;
          if (predicate(r, g, b)) matching = true;
          if (Math.abs(r - 47) <= 6 && Math.abs(g - 129) <= 6 && Math.abs(b - 54) <= 6) backgroundColorHit = true;
        }
      }
      return { opaque, matching, backgroundColorHit, pixels };
    }
    const isRock = (r, g, b) => Math.abs(r - g) < 15 && Math.abs(g - b) < 15 && r < 160;
    const isFlower = (r, g, b) => r > g + 40 && r > b + 40;
    const isTuft = (r, g, b) => g > r + 10 && g > b + 10;
    const rock = frameStats(DECORATION_TILES[0], isRock);
    const flower = frameStats(DECORATION_TILES[1], isFlower);
    const tuft = frameStats(DECORATION_TILES[2], isTuft);
    expect(rock.matching).toBe(true);
    expect(flower.matching).toBe(true);
    expect(tuft.matching).toBe(true);
    // Мелочь не занимает весь кадр 32×32 — остальное прозрачно (пропускает траву клетки).
    expect(rock.opaque).toBeLessThan(TILE * TILE);
    expect(flower.opaque).toBeLessThan(TILE * TILE);
    expect(tuft.opaque).toBeLessThan(TILE * TILE);

    // Требование 5 (ревью): в кадре мака нет точек цвета фона исходной вырезки — маскирование
    // сработало, а не просто скопировался прямоугольник целиком (прошло бы и со старым
    // `compositeRegion` без ключа).
    expect(flower.backgroundColorHit).toBe(false);

    // Требование 3 (ревью): камешек вырезан по форме — у прямоугольника кадра прозрачные углы,
    // не прямоугольная вырезка из середины валуна.
    const corners = [rock.pixels[0][0], rock.pixels[0][TILE - 1], rock.pixels[TILE - 1][0], rock.pixels[TILE - 1][TILE - 1]];
    for (const [, , , a] of corners) expect(a).toBe(0);
  });
});

describe("footprint против размера картинки (требование 14)", () => {
  it("footprint дерева и камня меньше их картинки", () => {
    expect(TREE_FOOTPRINT_SIZE[0]).toBeLessThan(TREE_IMAGE_SIZE[0]);
    expect(TREE_FOOTPRINT_SIZE[1]).toBeLessThan(TREE_IMAGE_SIZE[1]);
    expect(ROCK_FOOTPRINT_SIZE[0]).toBeLessThanOrEqual(ROCK_IMAGE_SIZE[0]);
  });
});

describe("CREDITS.txt и sources.json — каждая использованная часть и кусок атласа названы", () => {
  const sources = JSON.parse(readFileSync(resolve(lpcDir, "sources.json"), "utf8"));
  const credits = readFileSync(resolve(gameDir, "CREDITS.txt"), "utf8");

  it("каждая часть персонажей названа в CREDITS.txt", () => {
    for (const part of sources.parts) expect(credits).toContain(part.usedAs);
  });

  it("каждая таблица цветов названа в CREDITS.txt", () => {
    for (const palette of sources.palettes) expect(credits).toContain(palette.usedAs);
  });

  it("каждый кусок атласа назван, с авторами — или честно «не установлены», если не приписаны наугад", () => {
    for (const piece of sources.atlasPieces) {
      expect(credits).toContain(piece.usedAs);
      expect(credits).toContain(piece.authors.length > 0 ? piece.authors.join(", ") : "не установлены по атласу");
    }
  });

  it("CREDITS.txt не отсылает к Attribution.txt — авторов атласа называет сам", () => {
    expect(credits).not.toContain("Attribution.txt");
  });
});
