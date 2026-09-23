import { describe, expect, it } from "vitest";
import {
  buildObjectPropertiesView,
  parseSceneObjects,
  resolveSelectionAfterReload,
  summarizeSceneObjects,
} from "./sceneObjects";

describe("parseSceneObjects", () => {
  it("разбирает нормальный файл", () => {
    const text = JSON.stringify({ objects: [{ name: "a" }, { name: "b" }] });
    expect(parseSceneObjects(text)).toEqual([{ name: "a" }, { name: "b" }]);
  });

  it("отдаёт пустой список на битом JSON", () => {
    expect(parseSceneObjects("{не json")).toEqual([]);
  });

  it("отдаёт пустой список, когда нет objects", () => {
    expect(parseSceneObjects(JSON.stringify({ width: 10 }))).toEqual([]);
  });

  it("отдаёт пустой список, когда файл не прочитан", () => {
    expect(parseSceneObjects(null)).toEqual([]);
  });

  it("отдаёт пустой список, когда objects — не массив", () => {
    expect(parseSceneObjects(JSON.stringify({ objects: "нет" }))).toEqual([]);
  });
});

describe("summarizeSceneObjects", () => {
  it("нумерует по месту в файле и берёт name строкой", () => {
    const objects = [{ name: "голова" }, { name: "хвост" }];
    expect(summarizeSceneObjects(objects)).toEqual([
      { index: 0, name: "голова" },
      { index: 1, name: "хвост" },
    ]);
  });

  it("отдаёт null, когда name не строка или её нет", () => {
    const objects = [{ name: 5 }, {}, "не объект"];
    expect(summarizeSceneObjects(objects)).toEqual([
      { index: 0, name: null },
      { index: 1, name: null },
      { index: 2, name: null },
    ]);
  });
});

describe("buildObjectPropertiesView", () => {
  const objects = [{ position: [1, 2], color: "#e04040" }, "не объект", null];

  it("ничего не выбрано — none", () => {
    expect(buildObjectPropertiesView(objects, null)).toEqual({ status: "none" });
  });

  it("объекта с таким номером нет — none", () => {
    expect(buildObjectPropertiesView(objects, 10)).toEqual({ status: "none" });
  });

  it("объект — свойства по ключу в порядке файла, значение компактным JSON", () => {
    expect(buildObjectPropertiesView(objects, 0)).toEqual({
      status: "object",
      properties: [
        { key: "position", valueText: "[1,2]" },
        { key: "color", valueText: '"#e04040"' },
      ],
    });
  });

  it("элемент не объект — одной строкой его JSON", () => {
    expect(buildObjectPropertiesView(objects, 1)).toEqual({ status: "not-object", json: '"не объект"' });
  });

  it("элемент null — тоже одной строкой", () => {
    expect(buildObjectPropertiesView(objects, 2)).toEqual({ status: "not-object", json: "null" });
  });
});

describe("resolveSelectionAfterReload", () => {
  it("ничего не было выбрано — остаётся снят", () => {
    expect(resolveSelectionAfterReload(null, 5)).toBe(null);
  });

  it("объект с тем же номером ещё есть — выбор остаётся", () => {
    expect(resolveSelectionAfterReload(2, 5)).toBe(2);
  });

  it("объекта с таким номером больше нет — выбор снят", () => {
    expect(resolveSelectionAfterReload(4, 3)).toBe(null);
  });
});
