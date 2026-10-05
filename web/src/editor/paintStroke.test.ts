import { describe, expect, it } from "vitest";
import type { MaskSet } from "./maskBytes";
import { canPaintMaterial, newMaskPath, resolvePaintMaterial, topLayerIndex } from "./paintLayers";
import { applyPaintFrame, finishPaintStroke, paintStrokeLayers, paintStrokeMasks, startPaintStroke, type PaintFrame, type PaintStroke } from "./paintStroke";
import type { TerrainCoverLayer } from "./terrainFile";

/** Сцена 4 × 3 клетки. */
const SCENE = { width: 4, height: 3 };
/** Своя маска 8 × 6 — две точки на клетку, не четыре: красится в своём размере. Точка `(столбец, строка)` лежит в `((столбец + 0,5) / 2, (строка + 0,5) / 2)`. */
const MASK_WIDTH = 8;
const MASK_HEIGHT = 6;
const CENTER_OF_POINT_2_1: [number, number] = [1.25, 0.75];
const FRAME: PaintFrame = { size: 4, strength: 50, seconds: 0.1, isErasing: false };

function mask(fill: number): { width: number; height: number; pixels: Uint8Array } {
  return { width: MASK_WIDTH, height: MASK_HEIGHT, pixels: new Uint8Array(MASK_WIDTH * MASK_HEIGHT).fill(fill) };
}

function startStroke(covers: TerrainCoverLayer[] | null, masks: MaskSet, material: string, tintPath: string | null = null): PaintStroke {
  const stroke = startPaintStroke(covers, masks, SCENE, material, tintPath);
  if (stroke === null) throw new Error("мазок не начат");
  return stroke;
}

function byteAt(pixels: Uint8Array, column: number, row: number): number {
  return pixels[row * MASK_WIDTH + column] as number;
}

const GRASS_ROCK_SCREE: TerrainCoverLayer[] = [{ material: "grass" }, { material: "rock", mask: "terrain/rock.png" }, { material: "scree", mask: "terrain/scree.png" }];

describe("счёт кадра", () => {
  it("подъём маски: m ← 1 − (1 − m) × e^(−k × w × dt), k = сила / 10; в середине кисти w = 1", () => {
    const masks = { "terrain/rock.png": mask(0), "terrain/scree.png": mask(0) };
    const stroke = startStroke(GRASS_ROCK_SCREE, masks, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    const rock = paintStrokeMasks(stroke)[0]?.pixels as Uint8Array;
    expect(byteAt(rock, 2, 1)).toBe(Math.round(255 * (1 - Math.exp(-5 * 1 * 0.1))));
  });

  it("опускание маски слоя над материалом: m ← m × e^(−k × w × dt)", () => {
    const masks = { "terrain/rock.png": mask(0), "terrain/scree.png": mask(200) };
    const stroke = startStroke(GRASS_ROCK_SCREE, masks, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    const scree = paintStrokeMasks(stroke)[1]?.pixels as Uint8Array;
    expect(byteAt(scree, 2, 1)).toBe(Math.round(255 * (200 / 255) * Math.exp(-5 * 1 * 0.1)));
  });

  it("вес точки — по месту точки маски: w = (1 − (d / R)²)², d — расстояние до точки кисти по плоскости", () => {
    const stroke = startStroke(GRASS_ROCK_SCREE, { "terrain/rock.png": mask(0), "terrain/scree.png": mask(0) }, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    const rock = paintStrokeMasks(stroke)[0]?.pixels as Uint8Array;
    const weightOfNeighbour = (1 - (0.5 / 2) ** 2) ** 2;
    expect(byteAt(rock, 3, 1)).toBe(Math.round(255 * (1 - Math.exp(-5 * weightOfNeighbour * 0.1))));
    expect(byteAt(rock, 3, 1)).toBeLessThan(byteAt(rock, 2, 1));
  });

  it("меняются только точки маски внутри круга кисти", () => {
    const stroke = startStroke(GRASS_ROCK_SCREE, { "terrain/rock.png": mask(0), "terrain/scree.png": mask(0) }, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], { ...FRAME, size: 1 });

    const rock = paintStrokeMasks(stroke)[0]?.pixels as Uint8Array;
    const changed = Array.from(rock).flatMap((value, index) => (value === 0 ? [] : [index]));
    expect(changed).toEqual([1 * MASK_WIDTH + 2]);
  });

  it("круг частью за краем сцены красит только точки маски", () => {
    const stroke = startStroke(GRASS_ROCK_SCREE, { "terrain/rock.png": mask(0), "terrain/scree.png": mask(0) }, "rock");

    applyPaintFrame(stroke, [[0, 0]], { ...FRAME, size: 6 });

    const rock = paintStrokeMasks(stroke)[0]?.pixels as Uint8Array;
    expect(byteAt(rock, 0, 0)).toBeGreaterThan(0);
    expect(rock.length).toBe(MASK_WIDTH * MASK_HEIGHT);
  });

  it("секунды кадра делятся поровну между точками пути", () => {
    const masks = { "terrain/rock.png": mask(0), "terrain/scree.png": mask(0) };
    const whole = startStroke(GRASS_ROCK_SCREE, masks, "rock");
    const halves = startStroke(GRASS_ROCK_SCREE, masks, "rock");
    const first: [number, number] = [1, 0.75];

    applyPaintFrame(whole, [first, CENTER_OF_POINT_2_1], FRAME);
    applyPaintFrame(halves, [first], { ...FRAME, seconds: 0.05 });
    applyPaintFrame(halves, [CENTER_OF_POINT_2_1], { ...FRAME, seconds: 0.05 });

    expect(Array.from(paintStrokeMasks(whole)[0]?.pixels as Uint8Array)).toEqual(Array.from(paintStrokeMasks(halves)[0]?.pixels as Uint8Array));
  });

  it("кадр дольше 0,1 секунды считается за 0,1: вкладка в фоне не даёт огромного шага", () => {
    const masks = { "terrain/rock.png": mask(0), "terrain/scree.png": mask(0) };
    const long = startStroke(GRASS_ROCK_SCREE, masks, "rock");
    const capped = startStroke(GRASS_ROCK_SCREE, masks, "rock");

    applyPaintFrame(long, [CENTER_OF_POINT_2_1], { ...FRAME, seconds: 30 });
    applyPaintFrame(capped, [CENTER_OF_POINT_2_1], FRAME);

    expect(Array.from(paintStrokeMasks(long)[0]?.pixels as Uint8Array)).toEqual(Array.from(paintStrokeMasks(capped)[0]?.pixels as Uint8Array));
  });

  it("держишь дольше — гуще: байт растёт от кадра к кадру и подходит к 255", () => {
    const stroke = startStroke(GRASS_ROCK_SCREE, { "terrain/rock.png": mask(0), "terrain/scree.png": mask(0) }, "rock");
    const seen: number[] = [];

    for (let frame = 0; frame < 20; frame += 1) {
      applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);
      seen.push(byteAt(paintStrokeMasks(stroke)[0]?.pixels as Uint8Array, 2, 1));
    }

    expect(seen).toEqual([...seen].sort((first, second) => first - second));
    expect(seen.at(-1)).toBeGreaterThan(250);
  });
});

describe("правила слоёв: без Shift", () => {
  it("покрытий нет — первый кадр кладёт материал первым слоем без маски, на весь рельеф", () => {
    const stroke = startStroke(null, {}, "grass");

    expect(applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME)).toBe(true);

    expect(finishPaintStroke(stroke)).toEqual({ covers: [{ material: "grass" }], masks: {} });
  });

  it("слой выше первого без маски (только slope) получает чёрную маску terrain/<материал>.png в четыре точки на клетку", () => {
    const covers: TerrainCoverLayer[] = [{ material: "grass" }, { material: "scree", slope: 30 }];
    const stroke = startStroke(covers, {}, "scree");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], { ...FRAME, strength: 1, seconds: 0.0001 });

    const result = finishPaintStroke(stroke);
    expect(result?.covers).toEqual([{ material: "grass" }, { material: "scree", slope: 30, mask: "terrain/scree.png" }]);
    const created = result?.masks["terrain/scree.png"];
    expect([created?.width, created?.height]).toEqual([16, 12]);
    expect(created?.pixels.length).toBe(16 * 12);
  });

  it("материала нет среди слоёв и слоёв меньше восьми — наверх ложится новый слой с новой маской", () => {
    const stroke = startStroke([{ material: "grass" }], {}, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    const result = finishPaintStroke(stroke);
    expect(result?.covers).toEqual([{ material: "grass" }, { material: "rock", mask: "terrain/rock.png" }]);
    const created = result?.masks["terrain/rock.png"];
    expect(created?.pixels.some((value) => value > 0)).toBe(true);
    expect([created?.width, created?.height]).toEqual([16, 12]);
  });

  it("путь маски занят слоем другого материала — terrain/<материал>-2.png, дальше -3", () => {
    const covers: TerrainCoverLayer[] = [{ material: "grass" }, { material: "scree", mask: "terrain/rock.png" }, { material: "moss", mask: "terrain/rock-2.png" }];

    expect(newMaskPath(covers, "rock", null)).toBe("terrain/rock-3.png");
    expect(newMaskPath([{ material: "grass" }, { material: "scree", mask: "terrain/rock.png" }], "rock", null)).toBe("terrain/rock-2.png");
    expect(newMaskPath([{ material: "grass" }], "rock", null)).toBe("terrain/rock.png");
  });

  it("путь карты цвета tint тоже занят: маска нового слоя уходит в -2, дальше -3", () => {
    expect(newMaskPath([{ material: "grass" }], "rock", "terrain/rock.png")).toBe("terrain/rock-2.png");
    expect(newMaskPath([{ material: "grass" }, { material: "scree", mask: "terrain/rock-2.png" }], "rock", "terrain/rock.png")).toBe("terrain/rock-3.png");
    expect(newMaskPath([{ material: "grass" }], "rock", "terrain/tint.png")).toBe("terrain/rock.png");
  });

  it("новый слой не затирает карту цвета: маска ложится в -2", () => {
    const stroke = startStroke([{ material: "grass" }], {}, "rock", "terrain/rock.png");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    const result = finishPaintStroke(stroke);
    expect(result?.covers.at(-1)).toEqual({ material: "rock", mask: "terrain/rock-2.png" });
    expect(Object.keys(result?.masks ?? {})).toEqual(["terrain/rock-2.png"]);
  });

  it("новый слой с занятым путём кладёт маску в -2, не затирая чужую", () => {
    const covers: TerrainCoverLayer[] = [{ material: "grass" }, { material: "scree", mask: "terrain/rock.png" }];
    const stroke = startStroke(covers, { "terrain/rock.png": mask(77) }, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    const result = finishPaintStroke(stroke);
    expect(result?.covers.at(-1)).toEqual({ material: "rock", mask: "terrain/rock-2.png" });
    expect(Object.keys(result?.masks ?? {})).toEqual(["terrain/rock-2.png"]);
  });

  it("слои выше материала опускаются, слои ниже и слои без маски не меняются", () => {
    const covers: TerrainCoverLayer[] = [{ material: "grass" }, { material: "moss", mask: "terrain/moss.png" }, { material: "rock", mask: "terrain/rock.png" }, { material: "snow", slope: 40 }, { material: "scree", mask: "terrain/scree.png" }];
    const masks = { "terrain/moss.png": mask(255), "terrain/rock.png": mask(10), "terrain/scree.png": mask(255) };
    const stroke = startStroke(covers, masks, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    const [moss, rock, scree] = paintStrokeMasks(stroke).map((item) => byteAt(item.pixels, 2, 1));
    expect(moss).toBe(255);
    expect(rock).toBeGreaterThan(10);
    expect(scree).toBeLessThan(255);
    expect(paintStrokeLayers(stroke)).toEqual(covers);
  });

  it("материал верхнего слоя только поднимает его маску", () => {
    const stroke = startStroke(GRASS_ROCK_SCREE, { "terrain/rock.png": mask(255), "terrain/scree.png": mask(0) }, "scree");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    const [rock, scree] = paintStrokeMasks(stroke).map((item) => byteAt(item.pixels, 2, 1));
    expect([rock, scree]).toEqual([255, Math.round(255 * (1 - Math.exp(-0.5)))]);
  });

  it("материал первого слоя только опускает маски над ним", () => {
    const stroke = startStroke(GRASS_ROCK_SCREE, { "terrain/rock.png": mask(255), "terrain/scree.png": mask(255) }, "grass");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    const result = finishPaintStroke(stroke);
    expect(result?.covers).toEqual(GRASS_ROCK_SCREE);
    expect(Object.keys(result?.masks ?? {}).sort()).toEqual(["terrain/rock.png", "terrain/scree.png"]);
    expect(paintStrokeMasks(stroke).map((item) => byteAt(item.pixels, 2, 1)).every((value) => value < 255)).toBe(true);
  });

  it("материал лежит в двух слоях — красится верхний из них", () => {
    const covers: TerrainCoverLayer[] = [{ material: "grass" }, { material: "rock", mask: "terrain/a.png" }, { material: "moss", mask: "terrain/moss.png" }, { material: "rock", mask: "terrain/b.png" }];
    const masks = { "terrain/a.png": mask(0), "terrain/moss.png": mask(255), "terrain/b.png": mask(0) };
    expect(topLayerIndex(covers, "rock")).toBe(3);
    const stroke = startStroke(covers, masks, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    const [first, moss, top] = paintStrokeMasks(stroke).map((item) => byteAt(item.pixels, 2, 1));
    expect([first, moss]).toEqual([0, 255]);
    expect(top).toBeGreaterThan(0);
  });

  it("слоёв восемь и материала среди них нет — мазок ничего не делает", () => {
    const covers: TerrainCoverLayer[] = Array.from({ length: 8 }, (_, index) => (index === 0 ? { material: "grass" } : { material: `m${index}`, mask: `terrain/m${index}.png` }));
    const masks = Object.fromEntries(covers.slice(1).map((layer) => [layer.mask as string, mask(0)]));
    const stroke = startStroke(covers, masks, "rock");

    expect(canPaintMaterial(covers, "rock")).toBe(false);
    expect(canPaintMaterial(covers, "m3")).toBe(true);
    expect(applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME)).toBe(false);
    expect(finishPaintStroke(stroke)).toBeNull();
  });

  it("слой с маской, которой нет в наборе, — мазка нет", () => {
    expect(startPaintStroke(GRASS_ROCK_SCREE, { "terrain/rock.png": mask(0) }, SCENE, "rock", null)).toBeNull();
  });
});

describe("материал кисти", () => {
  it("выбранный, пока им можно красить", () => {
    expect(resolvePaintMaterial(["grass", "rock"], "rock", new Set())).toBe("rock");
  });

  it("ничего не выбрано или выбранный пропал из объявления — первый", () => {
    expect(resolvePaintMaterial(["grass", "rock"], null, new Set())).toBe("grass");
    expect(resolvePaintMaterial(["grass", "rock"], "moss", new Set())).toBe("grass");
  });

  it("слоёв восемь — первый, которым можно красить, а не заблокированный", () => {
    expect(resolvePaintMaterial(["grass", "rock", "scree"], null, new Set(["grass"]))).toBe("rock");
    expect(resolvePaintMaterial(["grass", "rock", "scree"], "grass", new Set(["grass"]))).toBe("rock");
  });

  it("материалов нет — кисти нечем красить", () => {
    expect(resolvePaintMaterial([], null, new Set())).toBe(undefined);
  });
});

describe("правила слоёв: с Shift", () => {
  const erasing: PaintFrame = { ...FRAME, isErasing: true };

  it("опускается маска слоя материала, остальные не меняются", () => {
    const masks = { "terrain/rock.png": mask(255), "terrain/scree.png": mask(255) };
    const stroke = startStroke(GRASS_ROCK_SCREE, masks, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], erasing);

    const [rock, scree] = paintStrokeMasks(stroke).map((item) => byteAt(item.pixels, 2, 1));
    expect(rock).toBe(Math.round(255 * Math.exp(-0.5)));
    expect(scree).toBe(255);
  });

  it("первый слой, слой без маски и материал, которого нет среди слоёв, — ничего не происходит", () => {
    const covers: TerrainCoverLayer[] = [{ material: "grass" }, { material: "rock", slope: 40 }, { material: "scree", mask: "terrain/scree.png" }];
    for (const material of ["grass", "rock", "water"]) {
      const stroke = startStroke(covers, { "terrain/scree.png": mask(255) }, material);

      expect(applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], erasing)).toBe(false);
      expect(finishPaintStroke(stroke)).toBeNull();
    }
  });

  it("Shift смотрится в каждом кадре: первый кадр красит, второй стирает", () => {
    const stroke = startStroke(GRASS_ROCK_SCREE, { "terrain/rock.png": mask(0), "terrain/scree.png": mask(0) }, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);
    const painted = byteAt(paintStrokeMasks(stroke)[0]?.pixels as Uint8Array, 2, 1);
    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], { ...FRAME, isErasing: true });

    expect(byteAt(paintStrokeMasks(stroke)[0]?.pixels as Uint8Array, 2, 1)).toBeLessThan(painted);
  });
});

describe("итог мазка", () => {
  it("маска стёрта до черноты — слой остаётся в файле", () => {
    const stroke = startStroke(GRASS_ROCK_SCREE, { "terrain/rock.png": mask(1), "terrain/scree.png": mask(0) }, "rock");

    for (let frame = 0; frame < 60; frame += 1) applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], { ...FRAME, strength: 100, isErasing: true });

    const result = finishPaintStroke(stroke);
    expect(result?.covers).toEqual(GRASS_ROCK_SCREE);
    expect(byteAt(result?.masks["terrain/rock.png"]?.pixels as Uint8Array, 2, 1)).toBe(0);
  });

  it("без изменений ни в байтах, ни в слоях — мазок не действие", () => {
    const stroke = startStroke(GRASS_ROCK_SCREE, { "terrain/rock.png": mask(255), "terrain/scree.png": mask(0) }, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    expect(finishPaintStroke(stroke)).toBeNull();
  });

  it("в итог входят только изменившиеся маски", () => {
    const stroke = startStroke(GRASS_ROCK_SCREE, { "terrain/rock.png": mask(0), "terrain/scree.png": mask(0) }, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    const result = finishPaintStroke(stroke);
    expect(Object.keys(result?.masks ?? {})).toEqual(["terrain/rock.png"]);
    expect(result?.covers).toEqual(GRASS_ROCK_SCREE);
  });

  it("байты исходных масок не меняются: к ним возвращает отмена", () => {
    const masks = { "terrain/rock.png": mask(0), "terrain/scree.png": mask(0) };
    const stroke = startStroke(GRASS_ROCK_SCREE, masks, "rock");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);
    finishPaintStroke(stroke);

    expect(masks["terrain/rock.png"].pixels.every((value) => value === 0)).toBe(true);
  });

  it("слои исходного списка не меняются", () => {
    const covers: TerrainCoverLayer[] = [{ material: "grass" }, { material: "scree", slope: 30 }];
    const stroke = startStroke(covers, {}, "scree");

    applyPaintFrame(stroke, [CENTER_OF_POINT_2_1], FRAME);

    expect(covers).toEqual([{ material: "grass" }, { material: "scree", slope: 30 }]);
  });
});
