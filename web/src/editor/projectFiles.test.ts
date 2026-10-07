import { describe, expect, it } from "vitest";
import {
  chooseParticlesFilePath,
  chooseTerrainFilePath,
  parseProjectCellPixels,
  parseProjectFilePaths,
  parseProjectImageDescriptions,
  parseProjectMaterialNames,
  parseProjectStamps,
} from "./projectFiles";

describe("parseProjectFilePaths", () => {
  it("путь рельефа — из files.terrain, без него null", () => {
    const withTerrain = '{ "files": { "scene": "scene.json", "properties": "properties.json", "terrain": "world/terrain.json" } }';
    const without = '{ "files": { "scene": "scene.json", "properties": "properties.json" } }';
    expect(parseProjectFilePaths(withTerrain)).toEqual({ scene: "scene.json", properties: "properties.json", terrain: "world/terrain.json", particles: null });
    expect(parseProjectFilePaths(without)).toEqual({ scene: "scene.json", properties: "properties.json", terrain: null, particles: null });
  });

  it("путь видов частиц — из files.particles, без него null", () => {
    const withParticles = '{ "files": { "scene": "scene.json", "properties": "properties.json", "particles": "fx/particles.json" } }';
    expect(parseProjectFilePaths(withParticles)?.particles).toBe("fx/particles.json");
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

describe("chooseParticlesFilePath", () => {
  it("particles.json рядом с game.json, если имя свободно", async () => {
    expect(await chooseParticlesFilePath(async () => false)).toBe("particles.json");
  });

  it("занятое имя — particles-2.json, particles-3.json и так далее, чужой файл не затирается", async () => {
    const taken = new Set(["particles.json", "particles-2.json"]);
    expect(await chooseParticlesFilePath(async (path) => taken.has(path))).toBe("particles-3.json");
  });
});

describe("parseProjectMaterialNames", () => {
  it("имена из files.materials в порядке объявления", () => {
    const text = '{ "files": { "materials": { "grass": { "size": 3 }, "rock": { "size": 8 }, "scree": {} } } }';

    expect(parseProjectMaterialNames(text)).toEqual(["grass", "rock", "scree"]);
  });

  it("без files.materials, не объект и непонятный текст — материалов нет", () => {
    expect(parseProjectMaterialNames('{ "files": {} }')).toEqual([]);
    expect(parseProjectMaterialNames('{ "files": { "materials": [] } }')).toEqual([]);
    expect(parseProjectMaterialNames('{ "files": { "materials": "x" } }')).toEqual([]);
    expect(parseProjectMaterialNames("{")).toEqual([]);
    expect(parseProjectMaterialNames(null)).toEqual([]);
  });
});

describe("parseProjectStamps", () => {
  it("имя и путь файла штампа, не строки пропускаются", () => {
    const text = '{ "files": { "stamps": { "beluha": "stamps/beluha.json", "bad": 5 } } }';

    expect(parseProjectStamps(text)).toEqual([{ name: "beluha", path: "stamps/beluha.json" }]);
  });

  it("без files.stamps — пусто", () => {
    expect(parseProjectStamps('{ "files": {} }')).toEqual([]);
    expect(parseProjectStamps(null)).toEqual([]);
  });
});

describe("parseProjectImageDescriptions", () => {
  it("картинки в порядке объявления с кадрами, столбцами, своим size и smooth", () => {
    const text = JSON.stringify({
      files: {
        images: {
          izba: { path: "images/izba.png" },
          hero: { path: "images/hero.png", frames: 60, columns: 15, size: [2, 2], smooth: true },
          ball: { path: "images/ball.png", frames: 4, frame_time: 0.15 },
        },
      },
    });

    expect(parseProjectImageDescriptions(text)).toEqual([
      { name: "izba", frames: null, columns: null, size: null, smooth: false },
      { name: "hero", frames: 60, columns: 15, size: [2, 2], smooth: true },
      { name: "ball", frames: 4, columns: null, size: null, smooth: false },
    ]);
  });

  it("описание не объект и неверные поля — умолчания, size не из двух положительных чисел — нет", () => {
    const text = '{ "files": { "images": { "a": 5, "b": { "size": [1], "frames": 0, "smooth": "yes" }, "c": { "size": [2, -1] } } } }';

    expect(parseProjectImageDescriptions(text)).toEqual([
      { name: "a", frames: null, columns: null, size: null, smooth: false },
      { name: "b", frames: null, columns: null, size: null, smooth: false },
      { name: "c", frames: null, columns: null, size: null, smooth: false },
    ]);
  });

  it("без files.images, не объект и непонятный текст — картинок нет", () => {
    expect(parseProjectImageDescriptions('{ "files": {} }')).toEqual([]);
    expect(parseProjectImageDescriptions('{ "files": { "images": [] } }')).toEqual([]);
    expect(parseProjectImageDescriptions("{")).toEqual([]);
    expect(parseProjectImageDescriptions(null)).toEqual([]);
  });
});

describe("parseProjectCellPixels", () => {
  it("cell_pixels из scene — число больше нуля", () => {
    expect(parseProjectCellPixels('{ "scene": { "width": 160, "cell_pixels": 96 } }')).toBe(96);
    expect(parseProjectCellPixels('{ "scene": { "cell_pixels": 0.5 } }')).toBe(0.5);
  });

  it("нет ключа, не число или не больше нуля — null", () => {
    expect(parseProjectCellPixels('{ "scene": { "width": 160 } }')).toBeNull();
    expect(parseProjectCellPixels('{ "scene": { "cell_pixels": "96" } }')).toBeNull();
    expect(parseProjectCellPixels('{ "scene": { "cell_pixels": 0 } }')).toBeNull();
    expect(parseProjectCellPixels('{ "scene": { "cell_pixels": -1 } }')).toBeNull();
    expect(parseProjectCellPixels('{ "width": 160 }')).toBeNull();
    expect(parseProjectCellPixels("{")).toBeNull();
    expect(parseProjectCellPixels(null)).toBeNull();
  });
});
