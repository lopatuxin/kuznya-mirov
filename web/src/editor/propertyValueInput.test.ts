import { describe, expect, it } from "vitest";
import { parsePropertyValueInput } from "./propertyValueInput";

describe("parsePropertyValueInput", () => {
  it("разбирает JSON", () => {
    expect(parsePropertyValueInput("3")).toBe(3);
    expect(parsePropertyValueInput("true")).toBe(true);
    expect(parsePropertyValueInput("[1, 6]")).toEqual([1, 6]);
  });

  it("не JSON — берётся строкой", () => {
    expect(parsePropertyValueInput("#e04040")).toBe("#e04040");
    expect(parsePropertyValueInput("abc")).toBe("abc");
  });

  it("свойство вида text — набранное не проходит через JSON (Таблицы данных, требование 40)", () => {
    expect(parsePropertyValueInput("123", true)).toBe("123");
    expect(parsePropertyValueInput('"quoted"', true)).toBe('"quoted"');
    expect(parsePropertyValueInput("", true)).toBe("");
  });
});
