import { describe, expect, it } from "vitest";
import {
  imprintAsPlacement,
  imprintHandleGeometry,
  imprintOutline,
  imprintToEntry,
  newImprintEntry,
  parseStampShape,
  readImprint,
  scaledImprint,
  shiftedImprintCopy,
  translatedImprint,
  turnedImprint,
  type Imprint,
} from "./imprintGeometry";
import { pinholeCamera } from "./pinholeCamera";

const IMPRINT: Imprint = { stamp: "beluha", position: [10, 8], size: [20, 10], height: 6, rotation: 0 };

describe("отпечаток из файла", () => {
  it("читается: rotation по умолчанию 0", () => {
    expect(readImprint({ stamp: "beluha", position: [10, 8], size: [20, 10], height: 6 })).toEqual(IMPRINT);
    expect(readImprint({ stamp: "beluha", position: [10, 8], size: [20, 10], height: 6, rotation: 30 })?.rotation).toBe(30);
  });

  it("поле не того вида или не хватает — ручек нет (отпечаток назовёт ошибкой движок)", () => {
    expect(readImprint(undefined)).toBe(null);
    expect(readImprint({ stamp: "beluha", position: [10, 8], size: [20, 10] })).toBe(null);
    expect(readImprint({ stamp: 5, position: [10, 8], size: [20, 10], height: 6 })).toBe(null);
    expect(readImprint({ stamp: "a", position: "abc", size: [20, 10], height: 6 })).toBe(null);
    expect(readImprint({ stamp: "a", position: [1, 2], size: [20, 10], height: 6, rotation: "x" })).toBe(null);
  });

  it("пишется числами до сотых, rotation — только не 0", () => {
    expect(imprintToEntry({ ...IMPRINT, position: [10.004, 8.126], height: 6.999 })).toEqual({ stamp: "beluha", position: [10, 8.13], size: [20, 10], height: 7 });
    expect(imprintToEntry({ ...IMPRINT, rotation: 345 })).toEqual({ stamp: "beluha", position: [10, 8], size: [20, 10], height: 6, rotation: 345 });
    expect(imprintToEntry({ ...IMPRINT, rotation: 0.001 })).not.toHaveProperty("rotation");
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

describe("новый отпечаток", () => {
  const shape = { name: "chuya", columns: 192, rows: 144 };

  it("середина — место на земле, глубина — по пропорции штампа, числа до сотых, rotation нет", () => {
    expect(newImprintEntry([40.126, 3.004], shape, 30, 10)).toEqual({ stamp: "chuya", position: [40.13, 3], size: [30, 22.5], height: 10 });
    expect(newImprintEntry([0, 0], { name: "a", columns: 3, rows: 2 }, 10, 4).size).toEqual([10, 6.67]);
  });

  it("высота пишется со знаком: −3 остаётся −3", () => {
    expect(newImprintEntry([5, 5], shape, 30, -3).height).toBe(-3);
    expect(imprintToEntry({ ...IMPRINT, height: -2.504 }).height).toBe(-2.5);
  });

  it("копия — на клетку правее, остальное то же", () => {
    expect(shiftedImprintCopy({ stamp: "a", position: [3, 4], size: [1, 1], height: 2, rotation: 5 })).toEqual({ stamp: "a", position: [4, 4], size: [1, 1], height: 2, rotation: 5 });
    expect(shiftedImprintCopy({ stamp: "a", position: "abc" })).toEqual({ stamp: "a", position: "abc" });
  });
});

describe("ручки отпечатка", () => {
  it("отпечаток для ручек — прямоугольник от угла: середина совпадает с position отпечатка, фигура с высотой", () => {
    const placement = imprintAsPlacement(IMPRINT);
    expect(placement.position).toEqual([0, 3]);
    expect(placement.hasShape).toBe(true);
    expect(placement.height).toBe(6);
  });

  it("у переноса нет вертикальной стрелки; у масштаба есть стрелка высоты, у поворота — кольцо", () => {
    const camera = pinholeCamera([10, 8], 0, 50, 40);
    expect(imprintHandleGeometry(camera.projection, IMPRINT, "translate", 0)?.tipZ).toBe(null);
    expect(imprintHandleGeometry(camera.projection, IMPRINT, "translate", 0)?.tipX).not.toBe(null);
    expect(imprintHandleGeometry(camera.projection, IMPRINT, "scale", 0)?.tipZ).not.toBe(null);
    expect(imprintHandleGeometry(camera.projection, IMPRINT, "rotate", 0)?.ring.length).toBeGreaterThan(0);
  });

  it("стрелка высоты: у поднимающего отпечатка вверх по экрану, у вдавливающего — вниз", () => {
    const camera = pinholeCamera([10, 8], 0, 50, 40);
    const raising = imprintHandleGeometry(camera.projection, IMPRINT, "scale", 0);
    const pressing = imprintHandleGeometry(camera.projection, { ...IMPRINT, height: -6 }, "scale", 0);
    expect(raising?.tipZ?.[1]).toBeLessThan(raising?.center[1] as number);
    expect(pressing?.tipZ?.[1]).toBeGreaterThan(pressing?.center[1] as number);
  });
});

describe("перенос, поворот и масштаб", () => {
  it("перенос: свободно — до сотой, по оси — одна координата, с Ctrl середина на целых клетках", () => {
    expect(translatedImprint(IMPRINT, [1.234, -0.5], "free", false).position).toEqual([11.23, 7.5]);
    expect(translatedImprint(IMPRINT, [1.234, -0.5], "x", false).position).toEqual([11.23, 8]);
    expect(translatedImprint(IMPRINT, [1.234, -0.5], "y", false).position).toEqual([10, 7.5]);
    expect(translatedImprint(IMPRINT, [1.4, 0.6], "free", true).position).toEqual([11, 9]);
  });

  it("поворот: на угол, на который ушло место под указателем; в отрезке от 0 до 360, с Ctrl кратно 15°", () => {
    // Точка справа от середины ушла вниз: угол вырос на 90°.
    expect(turnedImprint(IMPRINT, [20, 8], [10, 18], false).rotation).toBe(90);
    expect(turnedImprint({ ...IMPRINT, rotation: 350 }, [20, 8], [10, 18], false).rotation).toBe(80);
    expect(turnedImprint(IMPRINT, [20, 8], [10, -2], false).rotation).toBe(270);
    expect(turnedImprint(IMPRINT, [20, 8], [10 + 10 * Math.cos(0.3), 8 + 10 * Math.sin(0.3)], true).rotation).toBe(15);
  });

  it("масштаб: середина остаётся на месте, размеры и высота не меньше 0,1, с Ctrl доля до десятой", () => {
    const wider = scaledImprint(IMPRINT, "width", 1.5, false);
    expect(wider.size).toEqual([30, 10]);
    expect(wider.position).toEqual([10, 8]);
    expect(scaledImprint(IMPRINT, "uniform", 0.5, false)).toMatchObject({ size: [10, 5], height: 3, position: [10, 8] });
    expect(scaledImprint(IMPRINT, "height", 2, false)).toMatchObject({ size: [20, 10], height: 12 });
    expect(scaledImprint(IMPRINT, "depth", 0, false).size).toEqual([20, 0.1]);
    expect(scaledImprint(IMPRINT, "height", 0, false).height).toBe(0.1);
    expect(scaledImprint(IMPRINT, "width", 1.23, true).size[0]).toBe(24);
  });

  it("масштаб вдавливающего отпечатка сохраняет знак: −4 в доле 2 даёт −8, в доле 0,01 — −0,1", () => {
    const pressing: Imprint = { ...IMPRINT, height: -4 };
    expect(scaledImprint(pressing, "height", 2, false).height).toBe(-8);
    expect(scaledImprint(pressing, "height", 0.01, false).height).toBe(-0.1);
    expect(scaledImprint(pressing, "uniform", 0, false)).toMatchObject({ size: [0.1, 0.1], height: -0.1 });
    expect(scaledImprint(pressing, "width", 2, false).height).toBe(-4);
    expect(scaledImprint(pressing, "height", 1.23, true).height).toBe(-4.8);
  });

  it("масштаб повёрнутого отпечатка середину не двигает", () => {
    const scaled = scaledImprint({ ...IMPRINT, rotation: 40 }, "uniform", 1.3, false);
    expect(scaled.position).toEqual([10, 8]);
    expect(scaled.rotation).toBe(40);
  });
});

describe("рамка отпечатка", () => {
  it("четыре угла повёрнутого прямоугольника, а между ними точки не реже чем через клетку", () => {
    const outline = imprintOutline(IMPRINT);
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
    const outline = imprintOutline({ ...IMPRINT, rotation: 90 });
    expect(outline[0]?.[0]).toBeCloseTo(15, 9);
    expect(outline[0]?.[1]).toBeCloseTo(-2, 9);
  });

  it("сторона короче клетки — одна точка на сторону", () => {
    expect(imprintOutline({ ...IMPRINT, size: [0.5, 0.2] })).toHaveLength(4);
  });
});
