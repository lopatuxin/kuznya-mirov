import { describe, expect, it } from "vitest";
import { brushRingPoints } from "./brushCircle";

describe("brushRingPoints", () => {
  it("64 точки окружности радиуса в половину размера вокруг точки кисти", () => {
    const ring = brushRingPoints([5, 7], 2);
    expect(ring).toHaveLength(64);
    for (const [x, y] of ring) expect(Math.hypot(x - 5, y - 7)).toBeCloseTo(2, 9);
  });

  it("точки идут по кругу через равные углы, первая — справа от середины", () => {
    const ring = brushRingPoints([0, 0], 1);
    expect(ring[0]).toEqual([1, 0]);
    expect(ring[16]?.[0]).toBeCloseTo(0, 9);
    expect(ring[16]?.[1]).toBeCloseTo(1, 9);
    expect(ring[32]?.[0]).toBeCloseTo(-1, 9);
  });
});
