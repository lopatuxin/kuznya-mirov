import { describe, expect, it } from "vitest";
import { mountainsWithCopy, mountainsWithout, mountainsWithPlaced, mountainsWithReplaced, mountainsWithValue } from "./mountainEditing";
import type { MountainEntry } from "./terrainFile";

const FIRST: MountainEntry = { stamp: "beluha", position: [8, 0], size: [38, 32], height: 15 };
const SECOND: MountainEntry = { stamp: "chuya", position: [104, 4], size: [40, 30], height: 18, rotation: 345 };

describe("правка списка гор", () => {
  it("новая гора — в конец, выбрана", () => {
    expect(mountainsWithPlaced([FIRST], SECOND)).toEqual({ mountains: [FIRST, SECOND], selected: 1 });
    expect(mountainsWithPlaced([], FIRST)).toEqual({ mountains: [FIRST], selected: 0 });
  });

  it("замена — гора на своём месте, выбор не меняется; такой горы нет — действия нет", () => {
    const moved = { ...FIRST, position: [9, 1] };
    expect(mountainsWithReplaced([FIRST, SECOND], 0, moved)).toEqual({ mountains: [moved, SECOND] });
    expect(mountainsWithReplaced([FIRST], 3, moved)).toBe(null);
  });

  it("значение свойства: новое пишется, rotation без значения убирается, то же самое — не действие", () => {
    expect(mountainsWithValue([FIRST], 0, "height", 20)?.mountains[0]).toEqual({ ...FIRST, height: 20 });
    expect(mountainsWithValue([FIRST], 0, "rotation", 30)?.mountains[0]).toEqual({ ...FIRST, rotation: 30 });
    expect(mountainsWithValue([SECOND], 0, "rotation", undefined)?.mountains[0]).not.toHaveProperty("rotation");
    expect(mountainsWithValue([FIRST], 0, "height", 15)).toBe(null);
    expect(mountainsWithValue([FIRST], 0, "rotation", undefined)).toBe(null);
    expect(mountainsWithValue([FIRST], 2, "height", 1)).toBe(null);
  });

  it("свойство пишется как набрано: ошибку назовёт движок", () => {
    expect(mountainsWithValue([FIRST], 0, "size", "abc")?.mountains[0]).toEqual({ ...FIRST, size: "abc" });
  });

  it("копия — в конец, на клетку правее, выбрана; исходная не меняется", () => {
    const change = mountainsWithCopy([FIRST, SECOND], 1);
    expect(change?.mountains).toHaveLength(3);
    expect(change?.mountains[2]).toEqual({ ...SECOND, position: [105, 4] });
    expect(change?.mountains[1]).toBe(SECOND);
    expect(change?.selected).toBe(2);
    expect(mountainsWithCopy([FIRST], 4)).toBe(null);
  });

  it("удаление — горы после неё сдвигаются, выбор снят; такой горы нет — действия нет", () => {
    expect(mountainsWithout([FIRST, SECOND], 0)).toEqual({ mountains: [SECOND], selected: null });
    expect(mountainsWithout([FIRST], 0)).toEqual({ mountains: [], selected: null });
    expect(mountainsWithout([FIRST], 1)).toBe(null);
  });
});
