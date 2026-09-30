import { describe, expect, it } from "vitest";
import { chooseTerrainFilePath, parseProjectFilePaths } from "./projectFiles";

describe("parseProjectFilePaths", () => {
  it("путь рельефа — из files.terrain, без него null", () => {
    const withTerrain = '{ "files": { "scene": "scene.json", "properties": "properties.json", "terrain": "world/terrain.json" } }';
    const without = '{ "files": { "scene": "scene.json", "properties": "properties.json" } }';
    expect(parseProjectFilePaths(withTerrain)).toEqual({ scene: "scene.json", properties: "properties.json", terrain: "world/terrain.json" });
    expect(parseProjectFilePaths(without)).toEqual({ scene: "scene.json", properties: "properties.json", terrain: null });
  });

  it("сцены и свойств нет — правка недоступна", () => {
    expect(parseProjectFilePaths('{ "files": { "scene": "scene.json" } }')).toBe(null);
    expect(parseProjectFilePaths("{")).toBe(null);
    expect(parseProjectFilePaths(null)).toBe(null);
  });
});

describe("chooseTerrainFilePath", () => {
  it("terrain.json в папке scene.json, если имя свободно", async () => {
    expect(await chooseTerrainFilePath("scene.json", async () => false)).toBe("terrain.json");
    expect(await chooseTerrainFilePath("world/scene.json", async () => false)).toBe("world/terrain.json");
  });

  it("занятое имя — terrain-2.json, terrain-3.json и так далее, чужой файл не затирается", async () => {
    const taken = new Set(["terrain.json", "terrain-2.json"]);
    expect(await chooseTerrainFilePath("scene.json", async (path) => taken.has(path))).toBe("terrain-3.json");
  });
});
