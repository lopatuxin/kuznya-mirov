import { describe, expect, it } from "vitest";
import {
  mountainAsPlacement,
  mountainHandleGeometry,
  mountainOutline,
  mountainToEntry,
  newMountainEntry,
  parseStampShape,
  readMountain,
  scaledMountain,
  shiftedMountainCopy,
  translatedMountain,
  turnedMountain,
  type Mountain,
} from "./mountainGeometry";
import { pinholeCamera } from "./pinholeCamera";

const MOUNTAIN: Mountain = { stamp: "beluha", position: [10, 8], size: [20, 10], height: 6, rotation: 0 };

describe("гора из файла", () => {
  it("читается: rotation по умолчанию 0", () => {
    expect(readMountain({ stamp: "beluha", position: [10, 8], size: [20, 10], height: 6 })).toEqual(MOUNTAIN);
    expect(readMountain({ stamp: "beluha", position: [10, 8], size: [20, 10], height: 6, rotation: 30 })?.rotation).toBe(30);
  });

  it("поле не того вида или не хватает — ручек нет (гору назовёт ошибкой движок)", () => {
    expect(readMountain(undefined)).toBe(null);
    expect(readMountain({ stamp: "beluha", position: [10, 8], size: [20, 10] })).toBe(null);
    expect(readMountain({ stamp: 5, position: [10, 8], size: [20, 10], height: 6 })).toBe(null);
    expect(readMountain({ stamp: "a", position: "abc", size: [20, 10], height: 6 })).toBe(null);
    expect(readMountain({ stamp: "a", position: [1, 2], size: [20, 10], height: 6, rotation: "x" })).toBe(null);
  });

  it("пишется числами до сотых, rotation — только не 0", () => {
    expect(mountainToEntry({ ...MOUNTAIN, position: [10.004, 8.126], height: 6.999 })).toEqual({ stamp: "beluha", position: [10, 8.13], size: [20, 10], height: 7 });
    expect(mountainToEntry({ ...MOUNTAIN, rotation: 345 })).toEqual({ stamp: "beluha", position: [10, 8], size: [20, 10], height: 6, rotation: 345 });
    expect(mountainToEntry({ ...MOUNTAIN, rotation: 0.001 })).not.toHaveProperty("rotation");
  });
});

describe("штамп", () => {
  it("строки и точки в строке берутся из текста файла", () => {
    expect(parseStampShape("beluha", '{ "heights": [[0, 1, 0], [1, 0, 1]] }')).toEqual({ name: "beluha", columns: 3, rows: 2 });
  });

  it("не сетка, не JSON и ненайденный файл — null", () => {
    expect(parseStampShape("a", null)).toBe(null);
    expect(parseStampShape("a", "{")).toBe(null);
    expect(parseStampShape("a", '{ "heights": [] }')).toBe(null);
    expect(parseStampShape("a", '{ "heights": [[]] }')).toBe(null);
    expect(parseStampShape("a", "[]")).toBe(null);
  });
});

describe("новая гора", () => {
  const shape = { name: "chuya", columns: 192, rows: 144 };

  it("середина — место на земле, глубина — по пропорции штампа, числа до сотых, rotation нет", () => {
    expect(newMountainEntry([40.126, 3.004], shape, 30, 10)).toEqual({ stamp: "chuya", position: [40.13, 3], size: [30, 22.5], height: 10 });
    expect(newMountainEntry([0, 0], { name: "a", columns: 3, rows: 2 }, 10, 4).size).toEqual([10, 6.67]);
  });

  it("копия — на клетку правее, остальное то же", () => {
    expect(shiftedMountainCopy({ stamp: "a", position: [3, 4], size: [1, 1], height: 2, rotation: 5 })).toEqual({ stamp: "a", position: [4, 4], size: [1, 1], height: 2, rotation: 5 });
    expect(shiftedMountainCopy({ stamp: "a", position: "abc" })).toEqual({ stamp: "a", position: "abc" });
  });
});

describe("ручки горы", () => {
  it("гора для ручек — прямоугольник от угла: середина совпадает с position горы, фигура с высотой", () => {
    const placement = mountainAsPlacement(MOUNTAIN);
    expect(placement.position).toEqual([0, 3]);
    expect(placement.hasShape).toBe(true);
    expect(placement.height).toBe(6);
  });

  it("у переноса нет вертикальной стрелки; у масштаба есть стрелка высоты, у поворота — кольцо", () => {
    const camera = pinholeCamera([10, 8], 0, 50, 40);
    expect(mountainHandleGeometry(camera.projection, MOUNTAIN, "translate", 0)?.tipZ).toBe(null);
    expect(mountainHandleGeometry(camera.projection, MOUNTAIN, "translate", 0)?.tipX).not.toBe(null);
    expect(mountainHandleGeometry(camera.projection, MOUNTAIN, "scale", 0)?.tipZ).not.toBe(null);
    expect(mountainHandleGeometry(camera.projection, MOUNTAIN, "rotate", 0)?.ring.length).toBeGreaterThan(0);
  });
});

describe("перенос, поворот и масштаб", () => {
  it("перенос: свободно — до сотой, по оси — одна координата, с Ctrl середина на целых клетках", () => {
    expect(translatedMountain(MOUNTAIN, [1.234, -0.5], "free", false).position).toEqual([11.23, 7.5]);
    expect(translatedMountain(MOUNTAIN, [1.234, -0.5], "x", false).position).toEqual([11.23, 8]);
    expect(translatedMountain(MOUNTAIN, [1.234, -0.5], "y", false).position).toEqual([10, 7.5]);
    expect(translatedMountain(MOUNTAIN, [1.4, 0.6], "free", true).position).toEqual([11, 9]);
  });

  it("поворот: на угол, на который ушло место под указателем; в отрезке от 0 до 360, с Ctrl кратно 15°", () => {
    // Точка справа от середины ушла вниз: угол вырос на 90°.
    expect(turnedMountain(MOUNTAIN, [20, 8], [10, 18], false).rotation).toBe(90);
    expect(turnedMountain({ ...MOUNTAIN, rotation: 350 }, [20, 8], [10, 18], false).rotation).toBe(80);
    expect(turnedMountain(MOUNTAIN, [20, 8], [10, -2], false).rotation).toBe(270);
    expect(turnedMountain(MOUNTAIN, [20, 8], [10 + 10 * Math.cos(0.3), 8 + 10 * Math.sin(0.3)], true).rotation).toBe(15);
  });

  it("масштаб: середина остаётся на месте, размеры и высота не меньше 0,1, с Ctrl доля до десятой", () => {
    const wider = scaledMountain(MOUNTAIN, "width", 1.5, false);
    expect(wider.size).toEqual([30, 10]);
    expect(wider.position).toEqual([10, 8]);
    expect(scaledMountain(MOUNTAIN, "uniform", 0.5, false)).toMatchObject({ size: [10, 5], height: 3, position: [10, 8] });
    expect(scaledMountain(MOUNTAIN, "height", 2, false)).toMatchObject({ size: [20, 10], height: 12 });
    expect(scaledMountain(MOUNTAIN, "depth", 0, false).size).toEqual([20, 0.1]);
    expect(scaledMountain(MOUNTAIN, "height", 0, false).height).toBe(0.1);
    expect(scaledMountain(MOUNTAIN, "width", 1.23, true).size[0]).toBe(24);
  });

  it("масштаб повёрнутой горы середину не двигает", () => {
    const scaled = scaledMountain({ ...MOUNTAIN, rotation: 40 }, "uniform", 1.3, false);
    expect(scaled.position).toEqual([10, 8]);
    expect(scaled.rotation).toBe(40);
  });
});

describe("рамка горы", () => {
  it("четыре угла повёрнутого прямоугольника, а между ними точки не реже чем через клетку", () => {
    const outline = mountainOutline(MOUNTAIN);
    expect(outline).toHaveLength(20 + 10 + 20 + 10);
    expect(outline[0]).toEqual([0, 3]);
    expect(outline[20]).toEqual([20, 3]);
    expect(outline[30]).toEqual([20, 13]);
    expect(outline[50]).toEqual([0, 13]);
    for (const [index, point] of outline.entries()) {
      const next = outline[(index + 1) % outline.length] as readonly [number, number];
      expect(Math.hypot(next[0] - point[0], next[1] - point[1])).toBeLessThanOrEqual(1 + 1e-9);
    }
  });

  it("поворот на 90° — ширина идёт вдоль y: первый угол северо-восточный", () => {
    const outline = mountainOutline({ ...MOUNTAIN, rotation: 90 });
    expect(outline[0]?.[0]).toBeCloseTo(15, 9);
    expect(outline[0]?.[1]).toBeCloseTo(-2, 9);
  });

  it("сторона короче клетки — одна точка на сторону", () => {
    expect(mountainOutline({ ...MOUNTAIN, size: [0.5, 0.2] })).toHaveLength(4);
  });
});
