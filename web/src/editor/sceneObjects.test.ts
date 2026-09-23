import { describe, expect, it } from "vitest";
import {
  buildObjectPropertiesView,
  parseSceneObjects,
  parseSceneSize,
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
    expect(summarizeSceneObjects(objects).map(({ index, name }) => ({ index, name }))).toEqual([
      { index: 0, name: "голова" },
      { index: 1, name: "хвост" },
    ]);
  });

  it("отдаёт null, когда name не строка или её нет", () => {
    const objects = [{ name: 5 }, {}, "не объект"];
    expect(summarizeSceneObjects(objects).map(({ index, name }) => ({ index, name }))).toEqual([
      { index: 0, name: null },
      { index: 1, name: null },
      { index: 2, name: null },
    ]);
  });

  it("берёт цвет только в виде #rrggbb и картинку строкой, объект без position и size — не на сцене", () => {
    const objects = [
      { position: [0, 0], size: [1, 1], color: "#2b2f3a" },
      { position: [1, 1], size: [1, 1], image: "head" },
      { position: [2, 2], color: 5 },
      { score: 0, color: "red" },
    ];
    expect(summarizeSceneObjects(objects)).toEqual([
      { index: 0, name: null, color: "#2b2f3a", image: null, isOnScene: true },
      { index: 1, name: null, color: null, image: "head", isOnScene: true },
      { index: 2, name: null, color: null, image: null, isOnScene: false },
      { index: 3, name: null, color: null, image: null, isOnScene: false },
    ]);
  });
});

describe("parseSceneSize", () => {
  it("берёт ширину и высоту сцены из game.json", () => {
    expect(parseSceneSize(JSON.stringify({ scene: { width: 17, height: 27 } }))).toEqual({ width: 17, height: 27 });
  });

  it("файл не прочитан, не JSON или размера нет — null", () => {
    expect(parseSceneSize(null)).toBe(null);
    expect(parseSceneSize("{не json")).toBe(null);
    expect(parseSceneSize("null")).toBe(null);
    expect(parseSceneSize(JSON.stringify({ name: "x" }))).toBe(null);
    expect(parseSceneSize(JSON.stringify({ scene: { width: 0, height: 5 } }))).toBe(null);
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
