import { describe, expect, it } from "vitest";
import { strokeFrames } from "./strokePlayback";

const FRAME = 1 / 60;

describe("strokeFrames", () => {
  it("кадры по 1/60 секунды: секунда — 60 кадров, секунды кадров дают ровно секунду", () => {
    const frames = strokeFrames([[0, 0], [6, 0]], 1);

    expect(frames).toHaveLength(60);
    expect(frames.every((frame) => Math.abs(frame.seconds - FRAME) < 1e-12)).toBe(true);
    expect(frames.reduce((sum, frame) => sum + frame.seconds, 0)).toBeCloseTo(1, 12);
  });

  it("последний кадр короче", () => {
    const frames = strokeFrames([[0, 0], [1, 0]], 0.05);

    expect(frames).toHaveLength(3);
    expect(frames.at(-1)?.seconds).toBeCloseTo(0.05 - 2 * FRAME, 12);
    expect(frames.reduce((sum, frame) => sum + frame.seconds, 0)).toBeCloseTo(0.05, 12);
  });

  it("точка кадра — место на ломаной в конце его: постоянная скорость, последняя точка — в конце ломаной", () => {
    const frames = strokeFrames([[0, 0], [6, 0]], 1);

    expect(frames[0]?.point[0]).toBeCloseTo(0.1, 12);
    expect(frames[29]?.point[0]).toBeCloseTo(3, 12);
    expect(frames.at(-1)?.point).toEqual([6, 0]);
  });

  it("ломаная из нескольких звеньев идёт с одной скоростью по длине, а не по звеньям", () => {
    const frames = strokeFrames([[0, 0], [3, 0], [3, 9]], 1);

    // Длина 12; на середине (6) кисть уже на втором звене, в 3 клетках от угла.
    expect(frames[29]?.point[0]).toBeCloseTo(3, 12);
    expect(frames[29]?.point[1]).toBeCloseTo(3, 12);
    expect(frames.at(-1)?.point).toEqual([3, 9]);
  });

  it("одна точка — кисть стоит на месте все секунды", () => {
    const frames = strokeFrames([[52, 20]], 2);

    expect(frames).toHaveLength(120);
    expect(frames.every((frame) => frame.point[0] === 52 && frame.point[1] === 20)).toBe(true);
  });

  it("две одинаковые точки — тоже стоит на месте", () => {
    expect(strokeFrames([[5, 5], [5, 5]], 0.1).every((frame) => frame.point[0] === 5 && frame.point[1] === 5)).toBe(true);
  });

  it("очень короткий мазок — один кадр на всю секунду мазка", () => {
    const frames = strokeFrames([[0, 0], [1, 0]], 0.001);

    expect(frames).toHaveLength(1);
    expect(frames[0]?.seconds).toBeCloseTo(0.001, 12);
    expect(frames[0]?.point).toEqual([1, 0]);
  });
});
