import { describe, expect, it } from "vitest";
import { computeFlatScale, computeHeight, computeRotation, computeScale, computeTranslate, scaleFactorAlong, scaleFactorUniform } from "./handleMath";
import type { SpaceProjection } from "./handleGeometry";
import { placementCenter, type ObjectPlacement } from "./objectPlacement";
import { createVerticalGrab, type VerticalGrab } from "./verticalGrab";

const BOX: ObjectPlacement = { position: [6, 12], z: null, size: [3, 2], height: 2.5, rotation: null, hasShape: true };

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

describe("computeHeight", () => {
  // Параллельная проекция: 10 точек на клетку, вверх по экрану — вверх по высоте, `slope` — вбок на клетку высоты.
  const parallel = (slope: [number, number]): SpaceProjection => ({
    screenPoint: (x, y, z) => [x * 10 + 50 + z * slope[0], y * 10 + 50 + z * slope[1]],
  });
  const grabOf = (slope: [number, number]): VerticalGrab => createVerticalGrab(parallel(slope), [4, 5], 1) as VerticalGrab;
  const upright = grabOf([0, -10]);

  it("основание растёт на столько клеток, на сколько указатель ушёл вверх вдоль вертикали, до сотой", () => {
    expect(computeHeight(1, upright, [90, 50], [90, 18.7], false)).toBe(4.13);
    expect(computeHeight(1, upright, [90, 50], [90, 80], false)).toBe(-2);
  });

  it("вбок вертикаль не двигается; наклонная вертикаль берёт проекцию сдвига", () => {
    expect(computeHeight(2, upright, [90, 50], [200, 50], false)).toBe(2);
    expect(computeHeight(0, grabOf([6, -8]), [0, 0], [6, -8], false)).toBe(1);
  });

  it("с Ctrl — до целой клетки", () => {
    expect(computeHeight(0.2, upright, [90, 50], [90, 21], true)).toBe(3);
    expect(computeHeight(0.2, upright, [90, 50], [90, 46], true)).toBe(1);
  });

  it("вертикаль нулевой длины на экране (взгляд строго вниз) хвата не даёт", () => {
    expect(createVerticalGrab(parallel([0, 0]), [4, 5], 1)).toBeUndefined();
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
    const flat: ObjectPlacement = { position: [1, 1], z: null, size: [4, 1], height: null, rotation: null, hasShape: false };
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

describe("computeFlatScale", () => {
  const START = { position: [10, 15], size: [4, 2] } as const;

  it("правая сторона на +2 — size [6, 2], position не меняется", () => {
    expect(computeFlatScale(START, "right", [2, 0.7], false)).toEqual({ position: [10, 15], size: [6, 2] });
  });

  it("левая сторона на +1 — size [3, 2], position [11, 15]", () => {
    expect(computeFlatScale(START, "left", [1, 0], false)).toEqual({ position: [11, 15], size: [3, 2] });
  });

  it("верхняя сторона на −1 — size [4, 3], position [10, 14]", () => {
    expect(computeFlatScale(START, "top", [0, -1], false)).toEqual({ position: [10, 14], size: [4, 3] });
  });

  it("нижняя сторона меняет размер по y, position не меняется", () => {
    expect(computeFlatScale(START, "bottom", [3, 1.5], false)).toEqual({ position: [10, 15], size: [4, 3.5] });
  });

  it("правый нижний угол на (+2, +1): доля (24 + 6) / 20 = 1,5 — size [6, 3], position не меняется", () => {
    expect(computeFlatScale(START, "bottom-right", [2, 1], false)).toEqual({ position: [10, 15], size: [6, 3] });
  });

  it("левый верхний угол на (−2, −1) — size [6, 3], position [8, 14]", () => {
    expect(computeFlatScale(START, "top-left", [-2, -1], false)).toEqual({ position: [8, 14], size: [6, 3] });
  });

  it("правый верхний и левый нижний углы держат противоположный угол на месте", () => {
    expect(computeFlatScale(START, "top-right", [2, -1], false)).toEqual({ position: [10, 14], size: [6, 3] });
    expect(computeFlatScale(START, "bottom-left", [-2, 1], false)).toEqual({ position: [8, 15], size: [6, 3] });
  });

  it("указатель вбок от диагонали: берётся проекция на диагональ, (6·4 + 1·2) / 20 = 1,3", () => {
    expect(computeFlatScale(START, "bottom-right", [2, -1], false).size).toEqual([5.2, 2.6]);
  });

  it("правая сторона на −5 — size [0,1; 2], объект не отражается", () => {
    expect(computeFlatScale(START, "right", [-5, 0], false)).toEqual({ position: [10, 15], size: [0.1, 2] });
  });

  it("левая сторона за правой — size 0,1, правая сторона остаётся на месте", () => {
    const scaled = computeFlatScale(START, "left", [9, 0], false);

    expect(scaled.size[0]).toBe(0.1);
    expect(scaled.position[0]).toBeCloseTo(13.9);
  });

  it("у угла доля не меньше той, при которой меньшая сторона становится 0,1", () => {
    expect(computeFlatScale(START, "bottom-right", [-50, -50], false).size).toEqual([0.2, 0.1]);
  });

  it("свободно размеры округляются до сотой клетки", () => {
    expect(computeFlatScale(START, "right", [2.346, 0], false).size).toEqual([6.35, 2]);
  });

  it("с Ctrl доля 1,47 — 1,5", () => {
    expect(computeFlatScale(START, "right", [1.88, 0], true).size).toEqual([6, 2]);
    expect(computeFlatScale(START, "bottom-right", [1.88, 0.94], true).size).toEqual([6, 3]);
  });

  it("с Ctrl размер после доли тоже округляется до сотой клетки", () => {
    const start = { position: [10, 15], size: [13.33, 2] } as const;
    expect(computeFlatScale(start, "right", [2.67, 0], true).size).toEqual([16, 2]);
  });

  it("с Ctrl левая сторона держит правую на месте", () => {
    expect(computeFlatScale(START, "left", [-2.1, 0], true)).toEqual({ position: [8, 15], size: [6, 2] });
  });
});
