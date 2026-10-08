import { describe, expect, it } from "vitest";
import type { LoadedProjectImage } from "../projectLoader";
import type { ProjectImageDescription } from "./projectFiles";
import { buildImageTiles, createImageObject, createParticlesObject, imageFrameSize, newObjectParallax, newObjectSize } from "./projectImages";

function description(overrides: Partial<ProjectImageDescription> = {}): ProjectImageDescription {
  return { name: "izba", frames: null, columns: null, size: null, smooth: false, ...overrides };
}

function image(name: string, width: number, height: number): LoadedProjectImage {
  return { name, width, height, pixels: new Uint8Array(0) };
}

describe("imageFrameSize", () => {
  it("без frames — весь файл", () => {
    expect(imageFrameSize(image("a", 480, 288), description())).toEqual([480, 288]);
  });

  it("с frames без columns — кадры в одну строку: ширина файла / frames", () => {
    expect(imageFrameSize(image("a", 384, 96), description({ frames: 4 }))).toEqual([96, 96]);
  });

  it("с columns — сетка: ширина / columns, высота / строки, строки округляются вверх", () => {
    expect(imageFrameSize(image("a", 1440, 384), description({ frames: 60, columns: 15 }))).toEqual([96, 96]);
    expect(imageFrameSize(image("a", 300, 200), description({ frames: 7, columns: 3 }))).toEqual([100, 200 / 3]);
  });
});

describe("newObjectSize", () => {
  it("кадр в точках, делённый на cell_pixels: 480 × 288 при 96 — [5, 3]", () => {
    expect(newObjectSize([480, 288], description(), 96)).toEqual([5, 3]);
  });

  it("кадр 96 × 96 из сетки 60 кадров по 15 в ряд — [1, 1]", () => {
    expect(newObjectSize([96, 96], description({ frames: 60, columns: 15 }), 96)).toEqual([1, 1]);
  });

  it("без cell_pixels высота 1, ширина по пропорциям кадра: 480 × 288 — [1,67; 1]", () => {
    expect(newObjectSize([480, 288], description(), null)).toEqual([1.67, 1]);
  });

  it("свой size картинки важнее cell_pixels", () => {
    expect(newObjectSize([480, 288], description({ size: [2, 1.5] }), 96)).toEqual([2, 1.5]);
    expect(newObjectSize([480, 288], description({ size: [2, 1.5] }), null)).toEqual([2, 1.5]);
  });

  it("числа округляются до сотой клетки", () => {
    expect(newObjectSize([100, 100], description(), 96)).toEqual([1.04, 1.04]);
  });
});

describe("newObjectParallax", () => {
  it("parallax выбранного объекта; без выбора и без ключа — 1", () => {
    expect(newObjectParallax({ parallax: 0.6, layer: 20 })).toBe(0.6);
    expect(newObjectParallax({ layer: 20 })).toBe(1);
    expect(newObjectParallax(null)).toBe(1);
  });
});

describe("createImageObject", () => {
  it("пример требования 29: середина (48,5; 12,8), size [5, 3], выбран кусок слоя 20 с parallax 0,6", () => {
    const object = createImageObject("izba", [5, 3], [48.5, 12.8], { position: [1, 1], size: [4, 4], layer: 20, parallax: 0.6, color: "#fff", solid: true });

    expect(object).toEqual({ position: [46, 11.3], size: [5, 3], image: "izba", layer: 20, parallax: 0.6 });
    expect(Object.keys(object)).toEqual(["position", "size", "image", "layer", "parallax"]);
  });

  it("ничего не выбрано или у выбранного нет layer и parallax — их нет и у нового", () => {
    expect(createImageObject("izba", [5, 3], [10, 10], null)).toEqual({ position: [7.5, 8.5], size: [5, 3], image: "izba" });
    expect(createImageObject("izba", [5, 3], [10, 10], { position: [1, 1] })).toEqual({ position: [7.5, 8.5], size: [5, 3], image: "izba" });
  });

  it("у выбранного только один из ключей — берётся он", () => {
    expect(createImageObject("izba", [1, 1], [5, 5], { parallax: 0 })).toEqual({ position: [4.5, 4.5], size: [1, 1], image: "izba", parallax: 0 });
  });

  it("position округляется до сотой клетки", () => {
    expect(createImageObject("izba", [1.67, 1], [10.123, 5.456], null).position).toEqual([9.29, 4.96]);
  });
});

describe("createParticlesObject", () => {
  it("источник в одну клетку серединой под указателем, с layer и parallax выбранного — требование 34", () => {
    const object = createParticlesObject("smoke", 0.5, [48.5, 12.8], { position: [1, 1], layer: 20, parallax: 0.6, solid: true });

    expect(object).toEqual({ position: [48, 12.3], size: [1, 1], smoke: 0.5, layer: 20, parallax: 0.6 });
    expect(Object.keys(object)).toEqual(["position", "size", "smoke", "layer", "parallax"]);
  });

  it("ничего не выбрано — только position, size и главное свойство", () => {
    expect(createParticlesObject("sparks", 0.5, [10, 10], null)).toEqual({ position: [9.5, 9.5], size: [1, 1], sparks: 0.5 });
  });
});

describe("buildImageTiles", () => {
  it("картинки по порядку объявления; нет разжатых точек — image null", () => {
    const tiles = buildImageTiles([description({ name: "b" }), description({ name: "a" })], [image("a", 4, 4)]);

    expect(tiles.map((tile) => tile.description.name)).toEqual(["b", "a"]);
    expect(tiles[0]?.image).toBeNull();
    expect(tiles[1]?.image?.name).toBe("a");
  });
});
