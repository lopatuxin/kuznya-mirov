import { describe, expect, it } from "vitest";
import { parseSceneConfig } from "./sceneConfig";

describe("parseSceneConfig", () => {
  it("читает ширину и высоту сцены из game.json", () => {
    const gameJson = JSON.stringify({ scene: { width: 24, height: 24 } });
    expect(parseSceneConfig(gameJson)).toEqual({ width: 24, height: 24 });
  });

  it("возвращает запасной размер при неполном описании сцены", () => {
    const gameJson = JSON.stringify({ scene: { width: 24 } });
    const result = parseSceneConfig(gameJson);
    expect(result.width).toBeGreaterThan(0);
    expect(result.height).toBeGreaterThan(0);
  });

  it("не бросает исключение и отдаёт запасной размер при битом JSON", () => {
    expect(() => parseSceneConfig("{ битый json")).not.toThrow();
    expect(parseSceneConfig("{ битый json").width).toBeGreaterThan(0);
  });

  it("отдаёт запасной размер вместо нуля или отрицательного значения — иначе холст вырождается ещё до ошибки движка", () => {
    const zero = JSON.stringify({ scene: { width: 0, height: 24 } });
    const negative = JSON.stringify({ scene: { width: 24, height: -1 } });

    for (const gameJson of [zero, negative]) {
      expect(parseSceneConfig(gameJson)).toEqual({ width: 32, height: 24 });
    }
  });

  it("отдаёт запасной размер при переполнении числа в JSON (не конечное значение)", () => {
    const gameJson = '{"scene":{"width":1e400,"height":24}}';
    expect(parseSceneConfig(gameJson)).toEqual({ width: 32, height: 24 });
  });
});
