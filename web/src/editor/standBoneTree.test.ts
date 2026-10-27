import { describe, expect, it } from "vitest";
import { boneAncestors, shortBoneName, visibleBoneRows } from "./standBoneTree";

const BONES = [
  { name: "mixamorig:Hips", parent: -1 },
  { name: "mixamorig:Spine", parent: 0 },
  { name: "mixamorig:Head", parent: 1 },
  { name: "mixamorig:LeftUpLeg", parent: 0 },
];

describe("дерево костей", () => {
  it("видны корни и дети раскрытых костей, с глубиной", () => {
    expect(visibleBoneRows(BONES, new Set([0])).map(({ index, depth }) => [index, depth])).toEqual([
      [0, 0],
      [1, 1],
      [3, 1],
    ]);
    expect(visibleBoneRows(BONES, new Set([0, 1])).map(({ index }) => index)).toEqual([0, 1, 2, 3]);
    expect(visibleBoneRows(BONES, new Set()).map(({ hasChildren }) => hasChildren)).toEqual([true]);
  });

  it("предки кости — от корня к родителю", () => {
    expect(boneAncestors(BONES, 2)).toEqual([0, 1]);
    expect(boneAncestors(BONES, 0)).toEqual([]);
  });

  it("имя без префикса скелета", () => {
    expect(shortBoneName("mixamorig:LeftHand")).toBe("LeftHand");
    expect(shortBoneName("Hips")).toBe("Hips");
  });
});
