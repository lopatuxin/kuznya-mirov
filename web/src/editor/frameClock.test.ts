import { describe, expect, it } from "vitest";
import { MAX_FRAME_SECONDS, secondsSinceLastFrame } from "./frameClock";

describe("secondsSinceLastFrame", () => {
  it("первый кадр — ноль: времени с прошлого кадра ещё нет", () => {
    expect(secondsSinceLastFrame(null, 5000)).toBe(0);
  });

  it("кадр через 16 миллисекунд — 0,016 секунды", () => {
    expect(secondsSinceLastFrame(1000, 1016)).toBeCloseTo(0.016, 6);
  });

  it("спрятанная вкладка: кадр через пять секунд двигает часы не больше чем на 0,1", () => {
    expect(secondsSinceLastFrame(1000, 6000)).toBe(MAX_FRAME_SECONDS);
    expect(MAX_FRAME_SECONDS).toBe(0.1);
  });

  it("время, ушедшее назад, не даёт отрицательных секунд", () => {
    expect(secondsSinceLastFrame(2000, 1500)).toBe(0);
  });
});
