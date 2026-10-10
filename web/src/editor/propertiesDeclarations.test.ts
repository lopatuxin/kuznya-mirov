import { describe, expect, it } from "vitest";
import { defaultValueForPropertyKind, parsePropertyDeclarations, suggestPropertyNames } from "./propertiesDeclarations";

describe("parsePropertyDeclarations", () => {
  it("разбирает виды свойств автора", () => {
    expect(parsePropertyDeclarations('{"properties":{"score":"number","falling":"flag"}}')).toEqual({
      score: "number",
      falling: "flag",
    });
  });

  it("разбирает вид text", () => {
    expect(parsePropertyDeclarations('{"properties":{"catalogRow":"text"}}')).toEqual({ catalogRow: "text" });
  });

  it("отбрасывает неизвестный вид", () => {
    expect(parsePropertyDeclarations('{"properties":{"score":"string"}}')).toEqual({});
  });

  it("не разбирается — пустой список", () => {
    expect(parsePropertyDeclarations("не json")).toEqual({});
    expect(parsePropertyDeclarations(null)).toEqual({});
  });
});

describe("suggestPropertyNames", () => {
  it("даёт свойства движка и автора, которых нет у объекта", () => {
    const suggestions = suggestPropertyNames(["position", "size", "score"], { score: "number", hp: "flag" }, false);
    expect(suggestions).toContain("collides");
    expect(suggestions).toContain("deck");
    expect(suggestions).toContain("hp");
    expect(suggestions).not.toContain("position");
    expect(suggestions).not.toContain("score");
  });

  it("новые свойства движка Фазы 11 — среди подсказок, «+ свойство» не объявляет их автору", () => {
    const suggestions = suggestPropertyNames([], {}, false);
    expect(suggestions).toContain("camera_follows");
    expect(suggestions).toContain("walk_to");
    expect(suggestions).toContain("walk_speed");
    expect(suggestions).toContain("on_click");
  });

  it("flip_x Фазы 14 — среди подсказок", () => {
    expect(suggestPropertyNames([], {}, false)).toContain("flip_x");
  });

  it("parallax и repeat_x Фазы 28 — среди подсказок, пока их нет у объекта", () => {
    const suggestions = suggestPropertyNames([], {}, false);
    expect(suggestions).toContain("parallax");
    expect(suggestions).toContain("repeat_x");
    expect(suggestPropertyNames(["parallax", "repeat_x"], {}, false)).not.toContain("parallax");
    expect(suggestPropertyNames(["parallax", "repeat_x"], {}, false)).not.toContain("repeat_x");
  });

  it("sway Фазы 30 — среди подсказок плоской сцены, в трёхмерной его нет", () => {
    expect(suggestPropertyNames([], {}, false)).toContain("sway");
    expect(suggestPropertyNames([], {}, true)).not.toContain("sway");
    expect(suggestPropertyNames([], {}, true)).toContain("parallax");
    expect(suggestPropertyNames(["sway"], {}, false)).not.toContain("sway");
  });

  it("свойства частиц Фазы 32 и огня Фазы 35 — среди подсказок плоской сцены, в трёхмерной их нет", () => {
    expect(suggestPropertyNames([], {}, false)).toEqual(expect.arrayContaining(["smoke", "smoke_height", "smoke_color", "sparks", "sparks_reach", "sparks_direction", "sparks_spread", "leaf_fall", "leaf_color", "fire", "fire_color", "fire_glow"]));
    expect(suggestPropertyNames([], {}, true)).not.toContain("smoke");
    expect(suggestPropertyNames([], {}, true)).not.toContain("fire");
    expect(suggestPropertyNames(["leaf_fall"], {}, false)).not.toContain("leaf_fall");
  });

  it("clouds и cloud_images не предлагаются: их ставит группа «Облака» колонки «Свойства»", () => {
    const suggestions = suggestPropertyNames([], {}, false);
    expect(suggestions).not.toContain("clouds");
    expect(suggestions).not.toContain("cloud_images");
  });

  it("shape и height Фазы 15 — среди подсказок", () => {
    const suggestions = suggestPropertyNames([], {}, false);
    expect(suggestions).toContain("shape");
    expect(suggestions).toContain("height");
  });
});

describe("defaultValueForPropertyKind", () => {
  it("text — пустая строка (Таблицы данных, требование 39)", () => {
    expect(defaultValueForPropertyKind("text")).toBe("");
  });

  it("flag — true", () => {
    expect(defaultValueForPropertyKind("flag")).toBe(true);
  });
});
