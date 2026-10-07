import { describe, expect, it, vi } from "vitest";
import { applyWind, NO_WIND, parseSceneWind } from "./sceneWind";

describe("parseSceneWind", () => {
  it("читает wind корня", () => {
    expect(parseSceneWind('{ "wind": [1.5, -2], "objects": [] }')).toEqual([1.5, -2]);
  });

  it("нет ключа — ветра нет", () => {
    expect(parseSceneWind('{ "objects": [] }')).toEqual(NO_WIND);
  });

  it("не пара конечных чисел, не JSON и нет текста — ветра нет", () => {
    expect(parseSceneWind('{ "wind": [1] }')).toEqual(NO_WIND);
    expect(parseSceneWind('{ "wind": "east" }')).toEqual(NO_WIND);
    expect(parseSceneWind('{ "wind": [1, "a"] }')).toEqual(NO_WIND);
    expect(parseSceneWind("не json")).toEqual(NO_WIND);
    expect(parseSceneWind(null)).toEqual(NO_WIND);
  });
});

describe("applyWind", () => {
  it("зовёт set_wind_particles с парой чисел и при успехе ничего не возвращает", () => {
    const editor = { set_wind_particles: vi.fn(() => ({ ok: true })) };
    expect(applyWind(editor, [3, -1])).toBeUndefined();
    expect(editor.set_wind_particles).toHaveBeenCalledWith({ wind: [3, -1] });
  });

  it("возвращает текст ошибки движка", () => {
    const editor = { set_wind_particles: () => ({ ok: false, error: "ветер есть только в плоской сцене" }) };
    expect(applyWind(editor, [1, 0])).toBe("ветер есть только в плоской сцене");
  });
});
