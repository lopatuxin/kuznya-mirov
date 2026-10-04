import { describe, expect, it } from "vitest";
import { imprintsWithCopy, imprintsWithout, imprintsWithPlaced, imprintsWithReplaced, imprintsWithValue } from "./imprintEditing";
import type { ImprintEntry } from "./terrainFile";

const FIRST: ImprintEntry = { stamp: "beluha", position: [8, 0], size: [38, 32], height: 15 };
const SECOND: ImprintEntry = { stamp: "chuya", position: [104, 4], size: [40, 30], height: 18, rotation: 345 };

describe("правка списка отпечатков", () => {
  it("новый отпечаток — в конец, выбран", () => {
    expect(imprintsWithPlaced([FIRST], SECOND)).toEqual({ imprints: [FIRST, SECOND], selected: 1 });
    expect(imprintsWithPlaced([], FIRST)).toEqual({ imprints: [FIRST], selected: 0 });
  });

  it("замена — отпечаток на своём месте, выбор не меняется; такого отпечатка нет — действия нет", () => {
    const moved = { ...FIRST, position: [9, 1] };
    expect(imprintsWithReplaced([FIRST, SECOND], 0, moved)).toEqual({ imprints: [moved, SECOND] });
    expect(imprintsWithReplaced([FIRST], 3, moved)).toBe(null);
  });

  it("значение свойства: новое пишется, rotation без значения убирается, то же самое — не действие", () => {
    expect(imprintsWithValue([FIRST], 0, "height", 20)?.imprints[0]).toEqual({ ...FIRST, height: 20 });
    expect(imprintsWithValue([FIRST], 0, "rotation", 30)?.imprints[0]).toEqual({ ...FIRST, rotation: 30 });
    expect(imprintsWithValue([SECOND], 0, "rotation", undefined)?.imprints[0]).not.toHaveProperty("rotation");
    expect(imprintsWithValue([FIRST], 0, "height", 15)).toBe(null);
    expect(imprintsWithValue([FIRST], 0, "rotation", undefined)).toBe(null);
    expect(imprintsWithValue([FIRST], 2, "height", 1)).toBe(null);
  });

  it("свойство пишется как набрано: ошибку назовёт движок", () => {
    expect(imprintsWithValue([FIRST], 0, "size", "abc")?.imprints[0]).toEqual({ ...FIRST, size: "abc" });
  });

  it("копия — в конец, на клетку правее, выбрана; исходная не меняется", () => {
    const change = imprintsWithCopy([FIRST, SECOND], 1);
    expect(change?.imprints).toHaveLength(3);
    expect(change?.imprints[2]).toEqual({ ...SECOND, position: [105, 4] });
    expect(change?.imprints[1]).toBe(SECOND);
    expect(change?.selected).toBe(2);
    expect(imprintsWithCopy([FIRST], 4)).toBe(null);
  });

  it("удаление — отпечатки после него сдвигаются, выбор снят; такого отпечатка нет — действия нет", () => {
    expect(imprintsWithout([FIRST, SECOND], 0)).toEqual({ imprints: [SECOND], selected: null });
    expect(imprintsWithout([FIRST], 0)).toEqual({ imprints: [], selected: null });
    expect(imprintsWithout([FIRST], 1)).toBe(null);
  });
});
