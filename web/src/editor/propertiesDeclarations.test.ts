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
    const suggestions = suggestPropertyNames(["position", "size", "score"], { score: "number", hp: "flag" });
    expect(suggestions).toContain("collides");
    expect(suggestions).toContain("deck");
    expect(suggestions).toContain("hp");
    expect(suggestions).not.toContain("position");
    expect(suggestions).not.toContain("score");
  });

  it("новые свойства движка Фазы 11 — среди подсказок, «+ свойство» не объявляет их автору", () => {
    const suggestions = suggestPropertyNames([], {});
    expect(suggestions).toContain("camera_follows");
    expect(suggestions).toContain("walk_to");
    expect(suggestions).toContain("walk_speed");
    expect(suggestions).toContain("on_click");
  });

  it("flip_x Фазы 14 — среди подсказок", () => {
    expect(suggestPropertyNames([], {})).toContain("flip_x");
  });

  it("parallax и repeat_x Фазы 28 — среди подсказок, пока их нет у объекта", () => {
    const suggestions = suggestPropertyNames([], {});
    expect(suggestions).toContain("parallax");
    expect(suggestions).toContain("repeat_x");
    expect(suggestPropertyNames(["parallax", "repeat_x"], {})).not.toContain("parallax");
    expect(suggestPropertyNames(["parallax", "repeat_x"], {})).not.toContain("repeat_x");
  });

  it("shape и height Фазы 15 — среди подсказок", () => {
    const suggestions = suggestPropertyNames([], {});
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
