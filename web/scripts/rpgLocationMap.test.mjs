import { describe, expect, it } from "vitest";
import { CELL, parseLocationMap, extractRectangles } from "./rpgLocationMap.mjs";

describe("parseLocationMap", () => {
  it("разбирает строки карты в сетку", () => {
    const map = parseLocationMap("##\n..\n");
    expect(map).toEqual({ width: 2, height: 2, rows: ["##", ".."] });
  });

  it("падает на строках разной длины", () => {
    expect(() => parseLocationMap("###\n##\n")).toThrow(/разной длины/);
  });
});

describe("extractRectangles", () => {
  it("сливает горизонтальную полосу стены в один объект", () => {
    const map = parseLocationMap("####\n....\n");
    const rects = extractRectangles(map, CELL.WALL);
    expect(rects).toEqual([{ x: 0, y: 0, width: 4, height: 1 }]);
  });

  it("сливает вертикальную полосу стены в один объект", () => {
    const map = parseLocationMap("#.\n#.\n#.\n");
    const rects = extractRectangles(map, CELL.WALL);
    expect(rects).toEqual([{ x: 0, y: 0, width: 1, height: 3 }]);
  });

  it("сливает прямоугольный блок 3×2 в один объект, а не в полосы", () => {
    const map = parseLocationMap("###.\n###.\n....\n");
    const rects = extractRectangles(map, CELL.WALL);
    expect(rects).toEqual([{ x: 0, y: 0, width: 3, height: 2 }]);
  });

  it("не сливает клетки за пределами общего прямоугольника (уголок)", () => {
    // #.
    // .#
    const map = parseLocationMap("#.\n.#\n");
    const rects = extractRectangles(map, CELL.WALL);
    expect(rects).toHaveLength(2);
    expect(rects).toEqual(
      expect.arrayContaining([{ x: 0, y: 0, width: 1, height: 1 }, { x: 1, y: 1, width: 1, height: 1 }]),
    );
  });

  it("одиночная клетка — прямоугольник 1×1", () => {
    const map = parseLocationMap("...\n.#.\n...\n");
    const rects = extractRectangles(map, CELL.WALL);
    expect(rects).toEqual([{ x: 1, y: 1, width: 1, height: 1 }]);
  });

  it("Г-образная область — два прямоугольника, не один охватывающий блок", () => {
    // ##.
    // #..
    // #..
    const map = parseLocationMap("##.\n#..\n#..\n");
    const rects = extractRectangles(map, CELL.WALL);
    expect(rects).toHaveLength(2);
    const total = rects.reduce((sum, r) => sum + r.width * r.height, 0);
    expect(total).toBe(4); // 4 клетки стены всего, ни одна не посчитана дважды
  });
});
