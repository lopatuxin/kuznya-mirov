import { describe, expect, it } from "vitest";
import { parsePropertyDeclarations, suggestPropertyNames } from "./propertiesDeclarations";

describe("parsePropertyDeclarations", () => {
  it("разбирает виды свойств автора", () => {
    expect(parsePropertyDeclarations('{"properties":{"score":"number","falling":"flag"}}')).toEqual({
      score: "number",
      falling: "flag",
    });
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
    expect(suggestions).toContain("hp");
    expect(suggestions).not.toContain("position");
    expect(suggestions).not.toContain("score");
  });
});
