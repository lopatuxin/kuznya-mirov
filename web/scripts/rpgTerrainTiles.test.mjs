import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  NEIGHBOR_BIT,
  neighborBitmask,
  pathTileIndex,
  waterTileIndex,
  grassVariantTile,
  decorationTile,
  hasDecoration,
  GRASS_VARIANT_TILES,
  DECORATION_TILES,
  PATH_TILE_BASE,
  WATER_TILE_BASE,
} from "./rpgTerrainTiles.mjs";
import { parseLocationMap } from "./rpgLocationMap.mjs";

const scriptDir = fileURLToPath(new URL(".", import.meta.url));
const REAL_LOCATION_TEXT = readFileSync(resolve(scriptDir, "rpg-lpc/location.txt"), "utf8");

describe("neighborBitmask", () => {
  it("клетка без соседей своего вида — маска 0", () => {
    const mask = neighborBitmask(3, 3, 1, 1, () => false);
    expect(mask).toBe(0);
  });

  it("соседи с каждой стороны выставляют свой бит", () => {
    const same = new Set(["1,0", "2,1", "1,2", "0,1"]); // N, E, S, W от (1,1)
    const mask = neighborBitmask(3, 3, 1, 1, (x, y) => same.has(`${x},${y}`));
    expect(mask).toBe(NEIGHBOR_BIT.N | NEIGHBOR_BIT.E | NEIGHBOR_BIT.S | NEIGHBOR_BIT.W);
  });

  it("клетка на краю сцены не проверяет соседа за пределами сцены", () => {
    // (0,0): сосед сверху и слева не существуют — не должны считаться «своим видом».
    const mask = neighborBitmask(3, 3, 0, 0, () => true);
    expect(mask).toBe(NEIGHBOR_BIT.E | NEIGHBOR_BIT.S);
  });
});

describe("pathTileIndex / waterTileIndex", () => {
  it("индекс тропинки — маска плюс база 0", () => {
    expect(pathTileIndex(0)).toBe(PATH_TILE_BASE);
    expect(pathTileIndex(15)).toBe(PATH_TILE_BASE + 15);
  });

  it("индекс воды — маска плюс база 16, не пересекается с тропинкой", () => {
    expect(waterTileIndex(0)).toBe(WATER_TILE_BASE);
    for (let m = 0; m <= 15; m++) expect(waterTileIndex(m)).not.toBe(pathTileIndex(m));
  });
});

describe("grassVariantTile / decorationTile", () => {
  it("возвращают один из объявленных кадров варианта", () => {
    for (let x = 0; x < 10; x++) {
      for (let y = 0; y < 10; y++) {
        expect(GRASS_VARIANT_TILES).toContain(grassVariantTile(x, y));
        expect(DECORATION_TILES).toContain(decorationTile(x, y));
      }
    }
  });

  it("детерминированы: тот же результат при повторном вызове", () => {
    expect(grassVariantTile(5, 7)).toBe(grassVariantTile(5, 7));
    expect(decorationTile(5, 7)).toBe(decorationTile(5, 7));
  });
});

describe("grassVariantTile — распределение вариантов (требование 6)", () => {
  function primaryFraction(cells) {
    const primary = cells.filter(([x, y]) => grassVariantTile(x, y) === GRASS_VARIANT_TILES[0]).length;
    return primary / cells.length;
  }

  it("на реальной карте основной вариант — 75–95% клеток", () => {
    const map = parseLocationMap(REAL_LOCATION_TEXT);
    const cells = [];
    for (let y = 0; y < map.height; y++) for (let x = 0; x < map.width; x++) cells.push([x, y]);
    const fraction = primaryFraction(cells);
    expect(fraction).toBeGreaterThan(0.75);
    expect(fraction).toBeLessThan(0.95);
  });

  it("нет чередования через клетку: доля основного варианта одна и та же на чётных и нечётных x+y", () => {
    const map = parseLocationMap(REAL_LOCATION_TEXT);
    const evenCells = [];
    const oddCells = [];
    for (let y = 0; y < map.height; y++) {
      for (let x = 0; x < map.width; x++) {
        ((x + y) % 2 === 0 ? evenCells : oddCells).push([x, y]);
      }
    }
    const evenFraction = primaryFraction(evenCells);
    const oddFraction = primaryFraction(oddCells);
    // Прежний баг («чётность x^y») давал 0 на одной чётности и 1 на другой — разница должна быть
    // небольшой, не почти-единичной.
    expect(Math.abs(evenFraction - oddFraction)).toBeLessThan(0.1);
  });

  it("соседние по горизонтали клетки не отличаются каждый раз (не строгая шахматка)", () => {
    const map = parseLocationMap(REAL_LOCATION_TEXT);
    let differing = 0;
    let pairs = 0;
    for (let y = 0; y < map.height; y++) {
      for (let x = 0; x < map.width - 1; x++) {
        pairs++;
        if (grassVariantTile(x, y) !== grassVariantTile(x + 1, y)) differing++;
      }
    }
    expect(differing / pairs).toBeLessThan(0.5);
  });
});

describe("hasDecoration", () => {
  it("детерминированна и не отмечает каждую клетку", () => {
    let marked = 0;
    const total = 40 * 30;
    for (let x = 0; x < 40; x++) {
      for (let y = 0; y < 30; y++) {
        expect(hasDecoration(x, y)).toBe(hasDecoration(x, y));
        if (hasDecoration(x, y)) marked++;
      }
    }
    expect(marked).toBeGreaterThan(0);
    expect(marked).toBeLessThan(total);
  });
});
