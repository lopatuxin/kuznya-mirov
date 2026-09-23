import { describe, expect, it } from "vitest";
import { fitSceneStage } from "./sceneStageLayout";

describe("fitSceneStage", () => {
  it("квадратная сцена в широкой части окна — квадрат по высоте за вычетом полей", () => {
    expect(fitSceneStage(1400, 800, { width: 24, height: 24 }, 20)).toEqual({ width: 760, height: 760 });
  });

  it("высокая сцена упирается в высоту, ширина по пропорции", () => {
    expect(fitSceneStage(1000, 560, { width: 17, height: 27 }, 20)).toEqual({ width: 327, height: 520 });
  });

  it("широкая сцена в узкой части окна упирается в ширину", () => {
    expect(fitSceneStage(440, 900, { width: 24, height: 12 }, 20)).toEqual({ width: 400, height: 200 });
  });

  it("размер сцены неизвестен — всё место за вычетом полей", () => {
    expect(fitSceneStage(800, 600, null, 20)).toEqual({ width: 760, height: 560 });
  });

  it("часть окна меньше полей — холст не схлопывается в ноль", () => {
    expect(fitSceneStage(10, 10, { width: 24, height: 24 }, 20)).toEqual({ width: 1, height: 1 });
  });
});
