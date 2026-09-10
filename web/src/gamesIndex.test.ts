import { describe, expect, it } from "vitest";
import { parseGameDisplayName, parseGamesIndex } from "./gamesIndex";

describe("parseGamesIndex", () => {
  it("читает список имён папок игр", () => {
    expect(parseGamesIndex('["snake","arkanoid"]')).toEqual(["snake", "arkanoid"]);
  });

  it("бросает исключение на битом JSON", () => {
    expect(() => parseGamesIndex("{ битый json")).toThrow();
  });

  it("бросает исключение на пустом списке", () => {
    expect(() => parseGamesIndex("[]")).toThrow();
  });

  it("бросает исключение, если это не список", () => {
    expect(() => parseGamesIndex('{"snake":true}')).toThrow();
  });

  it("бросает исключение, если список содержит не только непустые строки", () => {
    expect(() => parseGamesIndex('["snake", ""]')).toThrow();
    expect(() => parseGamesIndex('["snake", 1]')).toThrow();
  });

  it("бросает исключение на имени с недопустимыми символами — тем же правилом, что и ?game=", () => {
    expect(() => parseGamesIndex('["snake 2"]')).toThrow();
    expect(() => parseGamesIndex('["my.game"]')).toThrow();
    expect(() => parseGamesIndex('["игра"]')).toThrow();
  });
});

describe("parseGameDisplayName", () => {
  it("читает название игры из поля name", () => {
    expect(parseGameDisplayName(JSON.stringify({ name: "Змейка" }))).toBe("Змейка");
  });

  it("бросает исключение на битом JSON", () => {
    expect(() => parseGameDisplayName("{ битый json")).toThrow();
  });

  it("бросает исключение при отсутствующем или пустом поле name", () => {
    expect(() => parseGameDisplayName("{}")).toThrow();
    expect(() => parseGameDisplayName(JSON.stringify({ name: "" }))).toThrow();
  });
});
