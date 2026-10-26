import { describe, expect, it } from "vitest";
import { bendThroughPoint, maxBend, MAX_BEND_DEGREES } from "./roadBendMath";

/** Дорога вдоль x от (0, 10) до (20, 10): положительный изгиб прогибает ось к меньшему y. */
const START: [number, number] = [0, 10];
const END: [number, number] = [20, 10];

/** Точка дуги радиуса `radius` с углом изгиба `bend` градусов — она лежит на дуге через начало и конец. */
function middleOfArc(bend: number): [number, number] {
  const radians = (Math.abs(bend) * Math.PI) / 180;
  const radius = 20 / (2 * Math.sin(radians / 2));
  const sagitta = radius * (1 - Math.cos(radians / 2));
  return [10, 10 - Math.sign(bend) * sagitta];
}

describe("bendThroughPoint", () => {
  it("середина дуги даёт тот изгиб, из которого она получена, с обеих сторон хорды", () => {
    for (const bend of [5, 30, 90, 150, -30, -90]) {
      expect(Math.abs(bendThroughPoint(START, END, middleOfArc(bend), 1) - bend)).toBeLessThan(0.02);
    }
  });

  it("точка дуги не посередине хорды тоже даёт изгиб дуги, что через неё проходит", () => {
    // Дуга изгиба 60° хорды 20: радиус 20, центр (10, 10 + 20 cos 30°); точка на ней при угле 20° от середины.
    const radius = 20;
    const center: [number, number] = [10, 10 + radius * Math.cos(Math.PI / 6)];
    const angle = -Math.PI / 2 + 0.3;
    const point: [number, number] = [center[0] + radius * Math.cos(angle), center[1] + radius * Math.sin(angle)];

    expect(bendThroughPoint(START, END, point, 1)).toBeCloseTo(60, 1);
  });

  it("ближе 1% длины хорды к хорде — 0, а чуть дальше — уже изгиб", () => {
    expect(bendThroughPoint(START, END, [10, 10.19], 1)).toBe(0);
    expect(bendThroughPoint(START, END, [10, 9.81], 1)).toBe(0);
    expect(bendThroughPoint(START, END, [10, 10], 1)).toBe(0);
    expect(bendThroughPoint(START, END, [10, 9.7], 1)).toBeGreaterThan(0);
    expect(bendThroughPoint(START, END, [10, 10.3], 1)).toBeLessThan(0);
  });

  it("точка на прямой хорды за её концом или хорда нулевой длины — 0, не NaN", () => {
    expect(bendThroughPoint(START, END, [40, 10], 1)).toBe(0);
    expect(bendThroughPoint(START, START, [3, 3], 1)).toBe(0);
  });

  it("дальше допустимого — ближайшее допустимое: меньше 180°", () => {
    expect(bendThroughPoint(START, END, [10, -50], 1)).toBe(MAX_BEND_DEGREES);
    expect(bendThroughPoint(START, END, [10, 50], 1)).toBe(-MAX_BEND_DEGREES);
    expect(bendThroughPoint(START, END, [10, 0], 1)).toBe(MAX_BEND_DEGREES);
  });

  it("радиус дуги не меньше ширины: широкая дорога гнётся меньше", () => {
    const limit = (2 * Math.asin(20 / 30) * 180) / Math.PI;
    const bend = bendThroughPoint(START, END, [10, 0], 15);

    expect(bend).toBeLessThanOrEqual(limit);
    expect(bend).toBeGreaterThan(limit - 0.011);
    expect(20 / (2 * Math.sin((bend * Math.PI) / 360))).toBeGreaterThanOrEqual(15);
  });

  it("изгиб считается до сотой доли градуса к нулю, не больше предела", () => {
    const bend = bendThroughPoint(START, END, [10, 4], 2);

    expect(bend).toBe(123.85);
    expect(Math.round(bend * 100)).toBe(bend * 100);
  });

  it("ход хорды сверху вниз: положительная сторона — вправо по ходу, как у поворота", () => {
    // Хорда идёт к большему y; сторона `(dy, -dx)` = `(1, 0)` — к большему x.
    expect(bendThroughPoint([10, 0], [10, 20], [14, 10], 1)).toBeGreaterThan(0);
    expect(bendThroughPoint([10, 0], [10, 20], [6, 10], 1)).toBeLessThan(0);
  });
});

describe("maxBend", () => {
  it("узкая дорога — предел 179,9; широкая — по радиусу не меньше ширины", () => {
    expect(maxBend(20, 2)).toBe(MAX_BEND_DEGREES);
    expect(maxBend(20, 15)).toBeCloseTo(83.62, 2);
    expect(maxBend(20, 10)).toBe(MAX_BEND_DEGREES);
  });

  it("нет ширины или хорды — предел 179,9", () => {
    expect(maxBend(0, 2)).toBe(MAX_BEND_DEGREES);
    expect(maxBend(20, 0)).toBe(MAX_BEND_DEGREES);
  });
});
