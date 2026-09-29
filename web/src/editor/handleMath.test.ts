import { describe, expect, it } from "vitest";
import { computeRotation, computeScale, computeTranslate, scaleFactorAlong, scaleFactorUniform } from "./handleMath";
import { placementCenter, type ObjectPlacement } from "./objectPlacement";

const BOX: ObjectPlacement = { position: [6, 12], size: [3, 2], height: 2.5, rotation: null, hasShape: true };

describe("computeTranslate", () => {
  it("по стрелке меняется только её координата", () => {
    expect(computeTranslate([3, 4], [2.3456, 1.5], "x", false)).toEqual([5.35, 4]);
    expect(computeTranslate([3, 4], [2.3456, 1.5], "y", false)).toEqual([3, 5.5]);
  });

  it("свободно — обе координаты до сотой клетки, без хвоста машинного округления", () => {
    expect(computeTranslate([3.5, 5.33], [0.07, 0.1], "free", false)).toEqual([3.57, 5.43]);
  });

  it("с Ctrl — до целой клетки", () => {
    expect(computeTranslate([3.4, 4], [0.3, 1.2], "free", true)).toEqual([4, 5]);
    expect(computeTranslate([3.4, 4.4], [0.3, 1.2], "x", true)).toEqual([4, 4.4]);
  });
});

describe("computeRotation", () => {
  const center: [number, number] = [10, 10];

  it("указатель по часовой стрелке, если смотреть сверху, — rotation растёт", () => {
    // Ось y растёт к игроку: из точки справа от середины к точке под ней — по часовой стрелке.
    expect(computeRotation(0, center, [15, 10], [10, 15], false)).toBe(90);
    expect(computeRotation(0, center, [15, 10], [10, 5], false)).toBe(270);
  });

  it("свободно — целый градус", () => {
    expect(computeRotation(0, center, [15, 10], [15, 10.26], false)).toBe(3);
  });

  it("с Ctrl — кратно 15°", () => {
    expect(computeRotation(0, center, [15, 10], [15, 10.5], true)).toBe(0);
    expect(computeRotation(0, center, [15, 10], [15, 3.3], true)).toBe(300);
    expect(computeRotation(0, center, [15, 10], [14, 12.2], true) % 15).toBe(0);
  });

  it("rotation −90 повернули на 10° по часовой — пишется 280", () => {
    const tenDegrees = (10 * Math.PI) / 180;
    const now: [number, number] = [10 + 5 * Math.cos(tenDegrees), 10 + 5 * Math.sin(tenDegrees)];
    expect(computeRotation(-90, center, [15, 10], now, false)).toBe(280);
  });

  it("значение пишется от 0 до 360, 360 не включая", () => {
    expect(computeRotation(350, center, [15, 10], [10, 15], false)).toBe(80);
    expect(computeRotation(null, center, [15, 10], [15, 10], false)).toBe(0);
  });
});

describe("computeScale", () => {
  it("середина прямоугольника на земле остаётся на месте — position пересчитывается", () => {
    const scaled = computeScale(BOX, "width", 2, false);
    expect(scaled.size).toEqual([6, 2]);
    expect(placementCenter(scaled)).toEqual(placementCenter(BOX));
    expect(scaled.position).toEqual([4.5, 12]);
    expect(scaled.height).toBe(2.5);
  });

  it("повёрнутого объекта position считается так же, размер меняется вдоль его собственной оси", () => {
    const turned: ObjectPlacement = { ...BOX, rotation: 45 };
    const scaled = computeScale(turned, "width", 1.5, false);
    expect(scaled.size).toEqual([4.5, 2]);
    expect(placementCenter(scaled)).toEqual(placementCenter(turned));
    expect(scaled.rotation).toBe(45);
  });

  it("свободно размеры округляются до сотой клетки", () => {
    expect(computeScale(BOX, "depth", 1.2345, false).size).toEqual([3, 2.47]);
  });

  it("с Ctrl доля округляется до десятой", () => {
    const scaled = computeScale(BOX, "uniform", 1.26, true);
    expect(scaled.size).toEqual([3.9, 2.6]);
    expect(scaled.height).toBe(3.25);
  });

  it("размер не меньше 0,1 клетки: ручка, перетянутая через середину, останавливается", () => {
    const scaled = computeScale(BOX, "depth", -0.7, false);
    expect(scaled.size[1]).toBe(0.1);
    expect(computeScale(BOX, "uniform", 0, true).size).toEqual([0.1, 0.1]);
  });

  it("ручка высоты меняет только высоту", () => {
    const scaled = computeScale(BOX, "height", 2, false);
    expect(scaled.height).toBe(5);
    expect(scaled.size).toEqual(BOX.size);
    expect(scaled.position).toEqual(BOX.position);
  });

  it("у фигуры без height высота считается единицей и пишется после масштаба", () => {
    const withoutHeight: ObjectPlacement = { ...BOX, height: null };
    expect(computeScale(withoutHeight, "height", 1.5, false).height).toBe(1.5);
    expect(computeScale(withoutHeight, "width", 1.5, false).height).toBe(null);
  });

  it("у плоского объекта на земле высоты нет: общая ручка меняет только размер", () => {
    const flat: ObjectPlacement = { position: [1, 1], size: [4, 1], height: null, rotation: null, hasShape: false };
    const scaled = computeScale(flat, "uniform", 2, false);
    expect(scaled.size).toEqual([8, 2]);
    expect(scaled.height).toBe(null);
    expect(computeScale(flat, "height", 2, false)).toEqual(flat);
  });
});

describe("scaleFactorAlong", () => {
  it("расстояние указателя от середины вдоль ручки сейчас, делённое на расстояние в начале", () => {
    expect(scaleFactorAlong([100, 100], [90, 0], [190, 100], [280, 100])).toBe(2);
    expect(scaleFactorAlong([100, 100], [90, 0], [190, 100], [145, 130])).toBe(0.5);
  });

  it("указатель прошёл середину — доля отрицательная", () => {
    expect(scaleFactorAlong([100, 100], [90, 0], [190, 100], [55, 100])).toBe(-0.5);
  });

  it("ручка, что на экране в точку (взгляд вдоль неё), долю не меняет", () => {
    expect(scaleFactorAlong([100, 100], [0, 0], [190, 100], [280, 100])).toBe(1);
  });
});

describe("scaleFactorUniform", () => {
  it("вправо-вверх доля растёт, влево-вниз падает, на месте — единица", () => {
    expect(scaleFactorUniform([100, 100], [100, 100], 90)).toBe(1);
    expect(scaleFactorUniform([100, 100], [190, 100], 90)).toBeGreaterThan(1);
    expect(scaleFactorUniform([100, 100], [100, 190], 90)).toBeLessThan(1);
  });
});
