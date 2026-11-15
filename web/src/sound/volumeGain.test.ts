import { describe, expect, it } from "vitest";
import { volumeToGain } from "./volumeGain";

describe("volumeToGain", () => {
  it("ставит силу звука квадратом доли", () => {
    expect(volumeToGain(50)).toBe(0.25);
    expect(volumeToGain(100)).toBe(1);
    expect(volumeToGain(0)).toBe(0);
  });

  it("не выходит за 0 и 1 при числе вне 0–100", () => {
    expect(volumeToGain(-10)).toBe(0);
    expect(volumeToGain(250)).toBe(1);
  });
});
