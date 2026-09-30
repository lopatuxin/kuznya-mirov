import { describe, expect, it } from "vitest";
import type { Vec2 } from "./objectPlacement";
import {
  applyBrushFrame,
  brushPathPoints,
  brushWeight,
  BRUSH_SIZE_LIMITS,
  clampToLimits,
  parseNumberField,
  type BrushFrame,
  type BrushKind,
} from "./terrainBrush";
import type { TerrainGrid } from "./terrainFile";

/** Сцена 8 × 8 клеток: точек сетки 17 × 17, точка `(column, row)` лежит в месте `(column / 2, row / 2)`. */
function flatGrid(height = 0): TerrainGrid {
  return { columns: 17, rows: 17, heights: new Float64Array(17 * 17).fill(height) };
}

function indexOf(grid: TerrainGrid, x: number, y: number): number {
  return y * 2 * grid.columns + x * 2;
}

function frame(kind: BrushKind, extra: Partial<BrushFrame> = {}): BrushFrame {
  return { settings: { kind, size: 4, strength: 50 }, seconds: 0.1, isLowering: false, levelTarget: 0, ...extra };
}

function runFrames(grid: TerrainGrid, point: Vec2, count: number, brushFrame: BrushFrame): void {
  for (let index = 0; index < count; index += 1) applyBrushFrame(grid, [point], brushFrame);
}

describe("brushWeight", () => {
  it("в середине 1, на половине радиуса 0,5625, на краю и дальше 0", () => {
    expect(brushWeight(0, 2)).toBe(1);
    expect(brushWeight(1, 2)).toBeCloseTo(0.5625, 12);
    expect(brushWeight(2, 2)).toBe(0);
    expect(brushWeight(5, 2)).toBe(0);
  });
});

describe("«Поднять»", () => {
  it("при силе 50 середина кисти, стоящей на месте, за секунду поднимается на клетку", () => {
    const grid = flatGrid();
    runFrames(grid, [4, 4], 10, frame("raise"));
    expect(grid.heights[indexOf(grid, 4, 4)]).toBeCloseTo(1, 9);
  });

  it("при силе 100 — на две клетки, вес края растёт по весу точки", () => {
    const grid = flatGrid();
    runFrames(grid, [4, 4], 10, frame("raise", { settings: { kind: "raise", size: 4, strength: 100 } }));
    expect(grid.heights[indexOf(grid, 4, 4)]).toBeCloseTo(2, 9);
    expect(grid.heights[indexOf(grid, 5, 4)]).toBeCloseTo(2 * 0.5625, 9);
    expect(grid.heights[indexOf(grid, 6, 4)]).toBe(0);
  });

  it("с Shift опускает на столько же", () => {
    const grid = flatGrid();
    runFrames(grid, [4, 4], 10, frame("raise", { isLowering: true }));
    expect(grid.heights[indexOf(grid, 4, 4)]).toBeCloseTo(-1, 9);
  });

  it("кадр дольше 0,1 секунды считается за 0,1", () => {
    const grid = flatGrid();
    applyBrushFrame(grid, [[4, 4]], frame("raise", { seconds: 3 }));
    expect(grid.heights[indexOf(grid, 4, 4)]).toBeCloseTo(0.1, 9);
  });

  it("высоты ничем не ограничены", () => {
    const grid = flatGrid(100);
    runFrames(grid, [4, 4], 100, frame("raise", { settings: { kind: "raise", size: 4, strength: 100 } }));
    expect(grid.heights[indexOf(grid, 4, 4)]).toBeGreaterThan(120);
  });

  it("точки за краем сцены не меняются, а кисть у края меняет только точки внутри", () => {
    const grid = flatGrid();
    applyBrushFrame(grid, [[0, 0]], frame("raise", { settings: { kind: "raise", size: 64, strength: 50 } }));
    expect(grid.heights).toHaveLength(17 * 17);
    expect(grid.heights[0]).toBeGreaterThan(0);
    expect(grid.heights.every((height) => Number.isFinite(height))).toBe(true);
    const outside = flatGrid();
    applyBrushFrame(outside, [[-30, -30]], frame("raise"));
    expect(outside.heights.every((height) => height === 0)).toBe(true);
  });
});

describe("«Выровнять»", () => {
  it("ведёт точку к высоте нажатия: при силе 50 середина вдвое сокращает разрыв за 0,14 секунды", () => {
    const grid = flatGrid();
    runFrames(grid, [4, 4], 14, frame("level", { levelTarget: 2, seconds: 0.01 }));
    expect(2 - (grid.heights[indexOf(grid, 4, 4)] as number)).toBeCloseTo(2 * Math.exp(-0.7), 9);
    expect((2 - (grid.heights[indexOf(grid, 4, 4)] as number)) / 2).toBeCloseTo(0.5, 2);
  });

  it("за секунду подходит к цели ближе чем на 1% разрыва", () => {
    const grid = flatGrid();
    runFrames(grid, [4, 4], 10, frame("level", { levelTarget: 3 }));
    expect(3 - (grid.heights[indexOf(grid, 4, 4)] as number)).toBeLessThan(0.03);
  });

  it("выравнивает и вверх, и вниз", () => {
    const grid = flatGrid(5);
    runFrames(grid, [4, 4], 10, frame("level", { levelTarget: 1 }));
    expect(grid.heights[indexOf(grid, 4, 4)]).toBeLessThan(1.05);
    expect(grid.heights[indexOf(grid, 4, 4)]).toBeGreaterThan(1);
  });
});

describe("«Сгладить»", () => {
  it("тянет пик к среднему соседей", () => {
    const grid = flatGrid();
    grid.heights[indexOf(grid, 4, 4)] = 9;
    runFrames(grid, [4, 4], 10, frame("smooth"));
    expect(grid.heights[indexOf(grid, 4, 4)]).toBeLessThan(2);
    expect(grid.heights[indexOf(grid, 4.5, 4)]).toBeGreaterThan(0);
  });

  it("средние считаются по высотам до кадра — порядок обхода точек результат не меняет", () => {
    const first = flatGrid();
    first.heights[indexOf(first, 4, 4)] = 9;
    const second = { ...first, heights: Float64Array.from(first.heights) };
    const path: Vec2[] = [[3.5, 4], [4, 4], [4.5, 4]];
    applyBrushFrame(first, path, frame("smooth"));
    applyBrushFrame(second, [...path].reverse(), frame("smooth"));
    for (let index = 0; index < first.heights.length; index += 1) {
      expect(first.heights[index]).toBeCloseTo(second.heights[index] as number, 12);
    }
  });

  it("у угла сетки среднее берётся по существующим соседям — трём и самой точке", () => {
    const grid = flatGrid();
    grid.heights[0] = 4;
    grid.heights[1] = 8;
    applyBrushFrame(grid, [[0, 0]], frame("smooth", { seconds: 0.1, settings: { kind: "smooth", size: 1, strength: 100 } }));
    // Среднее угла: (4 + 8 + 0 + 0) / 4 = 3; вес угла 1, k = 10, за 0,1 с разрыв × e^-1.
    expect(grid.heights[0]).toBeCloseTo(3 + (4 - 3) * Math.exp(-1), 9);
  });

  it("ровную землю не меняет", () => {
    const grid = flatGrid(2);
    runFrames(grid, [4, 4], 10, frame("smooth"));
    expect(grid.heights.every((height) => Math.abs(height - 2) < 1e-12)).toBe(true);
  });
});

describe("brushPathPoints", () => {
  it("кисть сдвинулась не дальше четверти размера — одна точка, куда пришла", () => {
    expect(brushPathPoints([1, 1], [1.9, 1], 4)).toEqual([[1.9, 1]]);
    expect(brushPathPoints(null, [3, 3], 4)).toEqual([[3, 3]]);
  });

  it("быстрый увод режется на равные промежутки не длиннее четверти размера, начало в путь не входит", () => {
    const path = brushPathPoints([0, 0], [10, 0], 4);
    expect(path).toHaveLength(10);
    expect(path[0]).toEqual([1, 0]);
    expect(path.at(-1)).toEqual([10, 0]);
    const chain: Vec2[] = [[0, 0], ...path];
    for (let index = 1; index < chain.length; index += 1) {
      const from = chain[index - 1] as Vec2;
      const to = chain[index] as Vec2;
      expect(Math.hypot(to[0] - from[0], to[1] - from[1])).toBeLessThanOrEqual(1 + 1e-9);
    }
  });

  it("действие кадра делится поровну между точками пути", () => {
    const whole = flatGrid();
    applyBrushFrame(whole, [[4, 4]], frame("raise"));
    const split = flatGrid();
    applyBrushFrame(split, [[4, 4], [4, 4], [4, 4], [4, 4]], frame("raise"));
    expect(split.heights[indexOf(split, 4, 4)]).toBeCloseTo(whole.heights[indexOf(whole, 4, 4)] as number, 12);
  });
});

describe("поля размера и силы", () => {
  it("число прижимается к пределам", () => {
    expect(clampToLimits(0, BRUSH_SIZE_LIMITS)).toBe(1);
    expect(clampToLimits(500, BRUSH_SIZE_LIMITS)).toBe(64);
    expect(clampToLimits(7.5, BRUSH_SIZE_LIMITS)).toBe(7.5);
  });

  it("не число — null, запятая читается как точка", () => {
    expect(parseNumberField("abc")).toBe(null);
    expect(parseNumberField("  ")).toBe(null);
    expect(parseNumberField("Infinity")).toBe(null);
    expect(parseNumberField("12,5")).toBe(12.5);
    expect(parseNumberField(" -0.5 ")).toBe(-0.5);
  });
});
