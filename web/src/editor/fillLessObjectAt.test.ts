import { describe, expect, it } from "vitest";
import { fillLessObjectAt } from "./fillLessObjectAt";
import type { CanvasRect } from "./selectionDrawing";

const RECTS: Record<number, CanvasRect> = {
  0: { x: 0, y: 0, width: 100, height: 100 },
  1: { x: 40, y: 40, width: 12, height: 3 },
  2: { x: 30, y: 30, width: 40, height: 40 },
};

const PLACE = { position: [0, 0], size: [1, 1] };

function pick(x: number, y: number, objects: Record<number, Record<string, unknown> | null>): number | undefined {
  return fillLessObjectAt(x, y, Object.keys(objects).map(Number), (id) => objects[id] ?? null, (id) => RECTS[id]);
}

describe("fillLessObjectAt", () => {
  it("находит источник без картинки и цвета, а объект с картинкой под ним пропускает", () => {
    expect(pick(45, 41, { 0: { ...PLACE, image: "forge" }, 1: { ...PLACE, smoke: 0.5 } })).toBe(1);
  });

  it("мимо источника и мимо объектов с заливкой ничего не находит; правый и нижний края не входят", () => {
    const objects = { 0: { ...PLACE, image: "forge" }, 1: { ...PLACE, smoke: 0.5 } };

    expect(pick(10, 10, objects)).toBeUndefined();
    expect(pick(52, 41, objects)).toBeUndefined();
    expect(pick(45, 43, objects)).toBeUndefined();
    expect(pick(40, 40, objects)).toBe(1);
  });

  it("цвет тоже заливка, объект без места на сцене и без свойств пропускаются", () => {
    expect(pick(45, 41, { 0: { ...PLACE, color: "#fff" }, 1: null, 5: { ...PLACE, smoke: 0.5 } })).toBeUndefined();
  });

  it("из двух выше слой побольше, при равных слоях — тот, что в списке позже", () => {
    expect(pick(45, 41, { 1: { ...PLACE, smoke: 0.5, layer: 3 }, 2: { ...PLACE, sparks: 0.5, layer: 1 } })).toBe(1);
    expect(pick(45, 41, { 1: { ...PLACE, smoke: 0.5 }, 2: { ...PLACE, sparks: 0.5 } })).toBe(2);
  });

  it("объект с repeat_x или без position и size частицы нести не может — пропускается, будто его нет", () => {
    expect(pick(45, 41, { 1: { ...PLACE, smoke: 0.5 }, 2: { ...PLACE, repeat_x: true } })).toBe(1);
    expect(pick(45, 41, { 1: { ...PLACE, smoke: 0.5 }, 2: { size: [1, 1] } })).toBe(1);
    expect(pick(45, 41, { 1: { position: [0, 0] }, 2: { ...PLACE, repeat_x: true } })).toBeUndefined();
  });

  it("repeat_x: false движок считает отсутствующим — такой объект частицы нести может", () => {
    expect(pick(45, 41, { 1: { ...PLACE, smoke: 0.5 }, 2: { ...PLACE, repeat_x: false } })).toBe(2);
  });
});
