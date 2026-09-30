import { describe, expect, it } from "vitest";
import { diffPlacements, normalizeRotation, readObjectPlacement, type ObjectPlacement } from "./objectPlacement";

const WALL: ObjectPlacement = { position: [3, 4], z: null, size: [5, 0.5], height: 2, rotation: -90, hasShape: true };

describe("readObjectPlacement", () => {
  it("читает место, размер, высоту, поворот и признак фигуры из свойств в виде файла", () => {
    expect(readObjectPlacement({ position: [3, 4], size: [5, 0.5], height: 2, rotation: -90, shape: "box", color: "#8a8f94" })).toEqual(WALL);
  });

  it("высоты и поворота может не быть, плоский объект без shape", () => {
    expect(readObjectPlacement({ position: [1, 2], size: [4, 1], image: "grass" })).toEqual({
      position: [1, 2],
      z: null,
      size: [4, 1],
      height: null,
      rotation: null,
      hasShape: false,
    });
  });

  it("третье число position — высота основания («Рельеф», требование 6)", () => {
    expect(readObjectPlacement({ position: [3, 4, -2.1], size: [5, 0.5], shape: "box" })?.z).toBe(-2.1);
    expect(readObjectPlacement({ position: [3, 4], size: [5, 0.5], shape: "box" })?.z).toBe(null);
  });

  it("без position или size, с нечисловым значением или без свойств — ничего", () => {
    expect(readObjectPlacement({ position: [1, 2] })).toBe(null);
    expect(readObjectPlacement({ position: [1, "a"], size: [1, 1] })).toBe(null);
    expect(readObjectPlacement(null)).toBe(null);
  });
});

describe("normalizeRotation", () => {
  it("от 0 до 360, 360 не включая", () => {
    expect(normalizeRotation(-80)).toBe(280);
    expect(normalizeRotation(360)).toBe(0);
    expect(normalizeRotation(725)).toBe(5);
  });
});

describe("diffPlacements", () => {
  it("без изменений — пусто: отпускание без изменения значений не действие", () => {
    expect(diffPlacements(WALL, { ...WALL })).toEqual([]);
  });

  it("собирает только изменившиеся свойства по порядку файла, с прежними значениями", () => {
    const moved: ObjectPlacement = { ...WALL, position: [4, 4], size: [6, 0.5], height: 3 };
    expect(diffPlacements(WALL, moved)).toEqual([
      { key: "position", value: [4, 4], previous: [3, 4] },
      { key: "size", value: [6, 0.5], previous: [5, 0.5] },
      { key: "height", value: 3, previous: 2 },
    ]);
  });

  it("rotation −90 и 10° по часовой — пишется 280, прежнее значение хранится как было", () => {
    expect(diffPlacements(WALL, { ...WALL, rotation: 270 })).toEqual([]);
    expect(diffPlacements(WALL, { ...WALL, rotation: -80 })).toEqual([{ key: "rotation", value: 280, previous: -90 }]);
  });

  it("свойства не было — previous не задан, чтобы отмена его убрала", () => {
    const bare: ObjectPlacement = { ...WALL, height: null, rotation: null };
    expect(diffPlacements(bare, { ...bare, height: 1.5, rotation: 30 })).toEqual([
      { key: "height", value: 1.5, previous: undefined },
      { key: "rotation", value: 30, previous: undefined },
    ]);
  });

  it("высота фигуры без height, оставшаяся единицей, и поворот 0 без rotation — не изменения", () => {
    const bare: ObjectPlacement = { ...WALL, height: null, rotation: null };
    expect(diffPlacements(bare, { ...bare, height: 1, rotation: 0 })).toEqual([]);
  });

  it("третье число position: появилось, исчезло или стало другим — position изменился и при том же месте", () => {
    expect(diffPlacements(WALL, { ...WALL, z: 0.3 })).toEqual([{ key: "position", value: [3, 4, 0.3], previous: [3, 4] }]);
    const risen: ObjectPlacement = { ...WALL, z: 2 };
    expect(diffPlacements(risen, { ...risen, z: null })).toEqual([{ key: "position", value: [3, 4], previous: [3, 4, 2] }]);
    expect(diffPlacements(risen, { ...risen, position: [4, 4], z: 3 })).toEqual([{ key: "position", value: [4, 4, 3], previous: [3, 4, 2] }]);
    expect(diffPlacements(risen, { ...risen })).toEqual([]);
  });

  it("у плоского объекта высота не пишется", () => {
    const flat: ObjectPlacement = { position: [1, 1], z: null, size: [4, 1], height: null, rotation: null, hasShape: false };
    expect(diffPlacements(flat, { ...flat, size: [8, 2] })).toEqual([{ key: "size", value: [8, 2], previous: [4, 1] }]);
  });
});
