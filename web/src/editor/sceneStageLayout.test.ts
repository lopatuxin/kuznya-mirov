import { describe, expect, it } from "vitest";
import { fitSceneStage } from "./sceneStageLayout";

describe("fitSceneStage", () => {
  it("холст занимает всю часть окна за вычетом полей", () => {
    expect(fitSceneStage(800, 600, 20)).toEqual({ width: 760, height: 560 });
  });

  it("дробный размер части окна округляется вниз", () => {
    expect(fitSceneStage(800.7, 600.2, 20)).toEqual({ width: 760, height: 560 });
  });

  it("часть окна меньше полей — холст не схлопывается в ноль", () => {
    expect(fitSceneStage(10, 10, 20)).toEqual({ width: 1, height: 1 });
  });
});
