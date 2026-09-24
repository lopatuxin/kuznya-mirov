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
});
