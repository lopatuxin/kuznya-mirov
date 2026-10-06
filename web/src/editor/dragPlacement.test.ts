import { describe, expect, it } from "vitest";
import { hasCrossedDragThreshold } from "./dragPlacement";

describe("hasCrossedDragThreshold", () => {
  it("сдвиг в пределах 4 пикселей не начинает перенос", () => {
    expect(hasCrossedDragThreshold(2, 2)).toBe(false);
    expect(hasCrossedDragThreshold(4, 0)).toBe(false);
  });

  it("сдвиг дальше 4 пикселей начинает перенос", () => {
    expect(hasCrossedDragThreshold(5, 0)).toBe(true);
    expect(hasCrossedDragThreshold(3, 3)).toBe(true);
  });
});
