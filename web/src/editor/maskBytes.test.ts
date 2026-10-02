import { describe, expect, it } from "vitest";
import { areMaskSetsEqual, areMasksEqual, changedMaskPaths, type MaskBytes } from "./maskBytes";

function mask(values: number[], width = values.length, height = 1): MaskBytes {
  return { width, height, pixels: Uint8Array.from(values) };
}

describe("areMasksEqual", () => {
  it("одни и те же размер и байты — равны, хоть массивы разные", () => {
    expect(areMasksEqual(mask([1, 2, 3]), mask([1, 2, 3]))).toBe(true);
  });

  it("другой байт или другая раскладка — не равны", () => {
    expect(areMasksEqual(mask([1, 2, 3]), mask([1, 2, 4]))).toBe(false);
    expect(areMasksEqual(mask([1, 2, 3, 4], 4, 1), mask([1, 2, 3, 4], 2, 2))).toBe(false);
    expect(areMasksEqual(mask([1, 2]), mask([1, 2, 3]))).toBe(false);
  });
});

describe("changedMaskPaths", () => {
  it("путь, которого не было, и путь с другими байтами; совпавшие не входят", () => {
    const base = { "a.png": mask([1]), "b.png": mask([2]) };
    const target = { "a.png": mask([1]), "b.png": mask([3]), "c.png": mask([4]) };

    expect(changedMaskPaths(base, target)).toEqual(["b.png", "c.png"]);
  });

  it("у цели путей меньше — меняться нечему", () => {
    expect(changedMaskPaths({ "a.png": mask([1]) }, {})).toEqual([]);
  });
});

describe("areMaskSetsEqual", () => {
  it("равны, когда пути и байты те же; лишний путь с любой стороны — нет", () => {
    expect(areMaskSetsEqual({ "a.png": mask([1]) }, { "a.png": mask([1]) })).toBe(true);
    expect(areMaskSetsEqual({ "a.png": mask([1]) }, { "a.png": mask([1]), "b.png": mask([1]) })).toBe(false);
    expect(areMaskSetsEqual({ "a.png": mask([1]), "b.png": mask([1]) }, { "a.png": mask([1]) })).toBe(false);
    expect(areMaskSetsEqual({ "a.png": mask([1]) }, { "b.png": mask([1]) })).toBe(false);
  });
});
