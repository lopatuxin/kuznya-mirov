import { describe, expect, it } from "vitest";
import type { ProjectFileReader } from "../projectLoader";
import type { MaskBytes } from "./maskBytes";
import { createPaintSceneController, type PaintContext, type PaintSceneContext, type PaintSceneEngine } from "./paintSceneController";
import type { PaintResult } from "./paintStroke";
import { decodeMaskPng, encodeMaskPng } from "./pngCodec";
import { parseProjectFilePaths } from "./projectFiles";
import { runStrokes, type StrokeRunResult, type StrokeWrite, type TerrainReadingsFunction } from "./strokeRunner";
import { strokeFrames } from "./strokePlayback";
import { flatTerrainGrid, formatTerrainText, parseTerrainText, type TerrainCoverLayer } from "./terrainFile";

const SCENE_SIZE = { width: 4, height: 3 };
const MATERIALS = { grass: { size: 3 }, rock: { size: 8 }, scree: { size: 2.5 } };
const GAME_JSON = JSON.stringify({
  name: "Проба",
  scene: { ...SCENE_SIZE, camera: { pitch: 55 } },
  files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", terrain: "terrain.json", materials: MATERIALS },
});
const GAME_JSON_WITHOUT_TERRAIN = JSON.stringify({
  name: "Проба",
  scene: { ...SCENE_SIZE, camera: { pitch: 55 } },
  files: { properties: "properties.json", scene: "scene.json", rules: "rules.json", materials: MATERIALS },
});

function terrainText(covers: TerrainCoverLayer[] | null = [{ material: "grass" }], tweak: (heights: Float64Array) => void = () => {}): string {
  const grid = flatTerrainGrid(SCENE_SIZE);
  tweak(grid.heights);
  return formatTerrainText({ ...grid, water: null, covers });
}

type Project = { files: Map<string, string | Uint8Array>; reader: ProjectFileReader };

function projectOf(entries: Record<string, string | Uint8Array>): Project {
  const files = new Map(Object.entries(entries));
  const reader: ProjectFileReader = {
    readText: async (path) => {
      const value = files.get(path);
      return typeof value === "string" ? value : null;
    },
    readBinary: async (path) => {
      const value = files.get(path);
      return value instanceof Uint8Array ? value : null;
    },
  };
  return { files, reader };
}

/** `terrain_readings` движка в миниатюре: высоты — из файла, итоговая земля — они плюс `lift` (отпечатки). */
function readingsWithLift(lift: number): TerrainReadingsFunction {
  return (_game, terrain) => {
    const parsed = terrain === null ? null : parseTerrainText(terrain);
    const grid = flatTerrainGrid(SCENE_SIZE);
    const heights = parsed?.heights ?? grid.heights;
    return { density: 2, columns: grid.columns, rows: grid.rows, heights, effective: Float64Array.from(heights, (height) => height + lift), water: parsed?.water ?? null };
  };
}

const NO_LIFT = readingsWithLift(0);

function toJson(strokes: unknown[]): string {
  return JSON.stringify(strokes);
}

const RAISE = { brush: "raise", size: 4, strength: 50, seconds: 1, points: [[2, 1.5]] };
const PAINT_ROCK = { brush: "paint", material: "rock", size: 3, strength: 80, seconds: 0.5, points: [[1, 1], [3, 2]] };

function writtenBy(result: StrokeRunResult): StrokeWrite[] {
  if (result.status !== "ok") throw new Error(result.message);
  return result.writes;
}

function textWritten(writes: StrokeWrite[], path: string): string {
  const content = writes.find((write) => write.path === path)?.content;
  if (typeof content !== "string") throw new Error(`текст ${path} не записан`);
  return content;
}

async function maskWritten(writes: StrokeWrite[], path: string): Promise<MaskBytes> {
  const content = writes.find((write) => write.path === path)?.content;
  if (!(content instanceof Uint8Array)) throw new Error(`маска ${path} не записана`);
  return decodeMaskPng(content);
}

describe("runStrokes — ошибки: ничего не пишется", () => {
  const project = (): Project => projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText() });

  const strokeErrors: [string, unknown[], string][] = [
    ["не JSON", [], "файл мазков не разбирается как JSON"],
    ["не список", [], "файл мазков: ожидается список мазков"],
    ["мазок не объект", [RAISE, 5], "мазок 2: ожидается объект"],
    ["неизвестный ключ", [{ ...RAISE, speed: 1 }], "мазок 1 → speed: неизвестный ключ"],
    ["нет обязательного поля", [{ brush: "raise" }], "мазок 1 → size: обязательного поля нет"],
    ["erode", [{ ...RAISE, brush: "erode" }], "мазок 1 → brush: нужна одна из кистей"],
    ["size", [RAISE, { ...RAISE, size: 65 }], "мазок 2 → size: нужно число от 1 до 64"],
    ["strength", [{ ...RAISE, strength: 0 }], "мазок 1 → strength: нужно число от 1 до 100"],
    ["seconds", [{ ...RAISE, seconds: 0 }], "мазок 1 → seconds: нужно число больше нуля"],
    ["points", [{ ...RAISE, points: [] }], "мазок 1 → points: нужен непустой список пар чисел"],
    ["shift", [{ ...RAISE, shift: "yes" }], "мазок 1 → shift: нужно true или false"],
    ["у paint нет material", [{ ...PAINT_ROCK, material: undefined }], "мазок 1 → material: у paint нужна строка"],
    ["material не объявлен", [{ ...PAINT_ROCK, material: "lava" }], "мазок 1 → material: материала «lava» нет в files.materials"],
    ["material у кисти рельефа", [{ ...RAISE, material: "rock" }], "мазок 1 → material: материал есть только у paint"],
  ];

  for (const [name, strokes, expected] of strokeErrors) {
    it(`${name}: «${expected}»`, async () => {
      const text = name === "не JSON" ? "[{" : name === "не список" ? '{"a":1}' : toJson(strokes);

      const result = await runStrokes(project().reader, NO_LIFT, text);

      expect(result.status).toBe("error");
      expect(result.status === "error" && result.message.startsWith(expected)).toBe(true);
      expect("writes" in result).toBe(false);
    });
  }

  it("paint новым материалом при восьми слоях — ошибка с номером мазка", async () => {
    const covers: TerrainCoverLayer[] = Array.from({ length: 8 }, (_, index) => (index === 0 ? { material: "grass" } : { material: `m${index}`, mask: `terrain/m${index}.png` }));
    const mask = await encodeMaskPng({ width: 4, height: 3, pixels: new Uint8Array(12) });
    const entries: Record<string, string | Uint8Array> = { "game.json": GAME_JSON, "terrain.json": terrainText(covers) };
    for (const layer of covers.slice(1)) entries[layer.mask as string] = mask;

    const result = await runStrokes(projectOf(entries).reader, NO_LIFT, toJson([RAISE, PAINT_ROCK]));

    expect(result).toEqual({ status: "error", message: "мазок 2 → material: слоёв уже восемь, а материала «rock» среди них нет" });
  });

  it("нет game.json — называет файл", async () => {
    expect(await runStrokes(projectOf({}).reader, NO_LIFT, toJson([RAISE]))).toEqual({ status: "error", message: "game.json: файла нет" });
  });

  it("движок отказал по рельефу — его текст как есть, ошибки файла мазков после проекта", async () => {
    const failing: TerrainReadingsFunction = () => ({ error: "terrain.json → heights: нужна сетка 9 × 7" });

    expect(await runStrokes(project().reader, failing, toJson([RAISE]))).toEqual({ status: "error", message: "terrain.json → heights: нужна сетка 9 × 7" });
  });

  it("маска не читается: нет файла или не PNG — называет файл и причину", async () => {
    const covers: TerrainCoverLayer[] = [{ material: "grass" }, { material: "rock", mask: "terrain/rock.png" }];

    const missing = await runStrokes(projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText(covers) }).reader, NO_LIFT, toJson([RAISE]));
    const broken = await runStrokes(projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText(covers), "terrain/rock.png": Uint8Array.of(1, 2, 3) }).reader, NO_LIFT, toJson([RAISE]));

    expect(missing).toEqual({ status: "error", message: "terrain/rock.png: файла нет" });
    expect(broken).toEqual({ status: "error", message: "terrain/rock.png: это не PNG" });
  });

  it("ошибка во втором мазке не оставляет записанным первый", async () => {
    const result = await runStrokes(project().reader, NO_LIFT, toJson([RAISE, { ...RAISE, size: 100 }]));

    expect(result.status).toBe("error");
    expect("writes" in result).toBe(false);
  });

  it("пустой список — «Мазков нет», ничего не пишется", async () => {
    expect(await runStrokes(project().reader, NO_LIFT, "[]")).toEqual({ status: "ok", message: "Мазков нет", writes: [] });
  });

  it("scene.json и правила не читаются: команда проверяет только то, что читает сама", async () => {
    const result = await runStrokes(project().reader, NO_LIFT, toJson([RAISE]));

    expect(result.status).toBe("ok");
  });
});

describe("runStrokes — кисти рельефа", () => {
  it("одна точка — кисть стоит на месте все секунды: в середине подъём за секунду на силу / 50", async () => {
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText() });

    const writes = writtenBy(await runStrokes(project.reader, NO_LIFT, toJson([RAISE])));

    const written = parseTerrainText(textWritten(writes, "terrain.json"));
    const center = 3 * written!.columns + 4;
    expect(written?.heights[center]).toBe(1);
    expect(written?.heights[0]).toBe(0);
    expect(writes.map((write) => write.path)).toEqual(["terrain.json"]);
  });

  it("на отпечатке кисть лепит итоговую землю: в heights пишется та же разница, что на ровном месте", async () => {
    const plain = writtenBy(await runStrokes(projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText() }).reader, NO_LIFT, toJson([RAISE])));
    const onImprint = writtenBy(await runStrokes(projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText() }).reader, readingsWithLift(7.5), toJson([RAISE])));

    expect(textWritten(onImprint, "terrain.json")).toBe(textWritten(plain, "terrain.json"));
  });

  it("«Выровнять» ведёт итоговую землю к итоговой высоте первой точки: выровненный отпечаток файл не меняет", async () => {
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText() });
    const level = { brush: "level", size: 6, strength: 100, seconds: 1, points: [[2, 1.5], [3, 1.5]] };

    const result = await runStrokes(project.reader, readingsWithLift(7.5), toJson([level]));

    expect(result).toEqual({ status: "ok", message: "Мазки выполнены, изменений нет: ничего не записано", writes: [] });
  });

  it("«Выровнять» ведёт соседние бугры к итоговой высоте первой точки", async () => {
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText(null, (heights) => heights.fill(4, 3 * 9 + 5, 3 * 9 + 6)) });
    const level = { brush: "level", size: 8, strength: 100, seconds: 2, points: [[2, 1.5]] };

    const writes = writtenBy(await runStrokes(project.reader, NO_LIFT, toJson([level])));

    const written = parseTerrainText(textWritten(writes, "terrain.json"));
    expect(written?.heights[3 * 9 + 5]).toBeLessThan(0.1);
    expect(written?.heights[3 * 9 + 5]).toBeGreaterThanOrEqual(0);
  });

  it("мазки идут по порядку: второй видит землю после первого", async () => {
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText() });

    const twice = writtenBy(await runStrokes(project.reader, NO_LIFT, toJson([RAISE, RAISE])));

    expect(parseTerrainText(textWritten(twice, "terrain.json"))?.heights[3 * 9 + 4]).toBe(2);
  });

  it("мазок за краем сцены меняет только точки сетки в пределах сцены", async () => {
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText() });

    const writes = writtenBy(await runStrokes(project.reader, NO_LIFT, toJson([{ ...RAISE, points: [[-1, -1]], size: 8 }])));

    const written = parseTerrainText(textWritten(writes, "terrain.json"));
    expect(written?.heights.length).toBe(63);
    expect(written?.heights[0]).toBeGreaterThan(0);
  });

  it("Shift опускает «Поднять» и ничего не меняет у «Сгладить» на ровном", async () => {
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText() });

    const lowered = writtenBy(await runStrokes(project.reader, NO_LIFT, toJson([{ ...RAISE, shift: true }])));
    const smoothed = await runStrokes(project.reader, NO_LIFT, toJson([{ brush: "smooth", size: 4, strength: 50, seconds: 1, points: [[2, 1.5]], shift: true }]));

    expect(parseTerrainText(textWritten(lowered, "terrain.json"))?.heights[3 * 9 + 4]).toBe(-1);
    expect(smoothed.status === "ok" && smoothed.writes).toEqual([]);
  });

  it("сохраняет воду, покрытия и перенос строки файла", async () => {
    const original = formatTerrainText({ ...flatTerrainGrid(SCENE_SIZE), water: { level: -1, color: "#3f7fd0" }, covers: [{ material: "grass" }] }, "\r\n");
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": original });

    const text = textWritten(writtenBy(await runStrokes(project.reader, NO_LIFT, toJson([RAISE]))), "terrain.json");

    expect(text.includes("\r\n")).toBe(true);
    expect(parseTerrainText(text)?.water).toEqual({ level: -1, color: "#3f7fd0" });
    expect(parseTerrainText(text)?.covers).toEqual([{ material: "grass" }]);
  });
});

describe("runStrokes — «Покрасить»", () => {
  it("новый материал кладёт слой и маску в четыре точки на клетку: маска пишется PNG, рельеф — со слоем", async () => {
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText() });

    const writes = writtenBy(await runStrokes(project.reader, NO_LIFT, toJson([PAINT_ROCK])));

    expect(writes.map((write) => write.path)).toEqual(["terrain/rock.png", "terrain.json"]);
    expect(parseTerrainText(textWritten(writes, "terrain.json"))?.covers).toEqual([{ material: "grass" }, { material: "rock", mask: "terrain/rock.png" }]);
    const mask = await maskWritten(writes, "terrain/rock.png");
    expect([mask.width, mask.height]).toEqual([16, 12]);
    expect(mask.pixels.some((value) => value > 0)).toBe(true);
  });

  it("тот же мазок кодом редактора по тем же кадрам даёт те же байты маски", async () => {
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText() });
    const commanded = await maskWritten(writtenBy(await runStrokes(project.reader, NO_LIFT, toJson([PAINT_ROCK]))), "terrain/rock.png");

    const committed: PaintResult[] = [];
    const frames = strokeFrames(PAINT_ROCK.points as [number, number][], PAINT_ROCK.seconds);
    const engine = {
      terrain_at: (x: number, y: number) => [x / 10, y / 10, 0],
      terrain_heights: () => undefined,
      set_terrain: () => undefined,
      set_covers: () => undefined,
    } as unknown as PaintSceneEngine;
    const paint: PaintContext = {
      material: "rock",
      size: PAINT_ROCK.size,
      strength: PAINT_ROCK.strength,
      covers: [{ material: "grass" }],
      masks: {},
      sceneSize: SCENE_SIZE,
      hasTerrainFile: true,
      tintPath: null,
      onCommit: (result) => committed.push(result),
      onRestore: () => {},
    };
    const context: PaintSceneContext = { engine, paint, onStrokeActiveChange: () => {} };
    const controller = createPaintSceneController();
    const input = (point: readonly [number, number], timeStamp: number, buttons: number) => ({ pointerId: 1, button: 0, buttons, x: point[0] * 10, y: point[1] * 10, shiftKey: false, timeStamp });

    controller.start(context, input([1, 1], 0, 1));
    let now = 0;
    for (const frame of frames) {
      controller.pointerMove(input(frame.point, now, 1));
      now += frame.seconds * 1000;
      controller.frame(context, now);
    }
    controller.pointerUp(input(frames.at(-1)?.point as readonly [number, number], now, 0));

    const edited = committed[0]?.masks["terrain/rock.png"];
    expect(Array.from(edited?.pixels ?? [])).toEqual(Array.from(commanded.pixels));
  });

  it("материал слоя без маски (только slope) получает маску; Shift после этого стирает", async () => {
    const covers: TerrainCoverLayer[] = [{ material: "grass" }, { material: "scree", slope: 30 }];
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText(covers) });

    const writes = writtenBy(
      await runStrokes(project.reader, NO_LIFT, toJson([{ ...PAINT_ROCK, material: "scree" }, { ...PAINT_ROCK, material: "scree", shift: true, seconds: 0.2, points: [[1, 1]] }])),
    );

    expect(parseTerrainText(textWritten(writes, "terrain.json"))?.covers).toEqual([{ material: "grass" }, { material: "scree", slope: 30, mask: "terrain/scree.png" }]);
    expect(writes.map((write) => write.path)).toContain("terrain/scree.png");
  });

  it("пишутся только изменившиеся маски; существующая маска остаётся, где её не трогали", async () => {
    const covers: TerrainCoverLayer[] = [{ material: "grass" }, { material: "rock", mask: "terrain/rock.png" }, { material: "scree", mask: "terrain/scree.png" }];
    const blank = await encodeMaskPng({ width: 8, height: 6, pixels: new Uint8Array(48) });
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText(covers), "terrain/rock.png": blank, "terrain/scree.png": blank });

    const result = await runStrokes(project.reader, NO_LIFT, toJson([{ ...PAINT_ROCK, material: "scree", points: [[1, 1]] }]));

    const writes = writtenBy(result);
    expect(writes.map((write) => write.path)).toEqual(["terrain/scree.png"]);
    expect((await maskWritten(writes, "terrain/scree.png")).width).toBe(8);
  });

  it("мазок материалом без изменений не пишет ничего", async () => {
    const covers: TerrainCoverLayer[] = [{ material: "grass" }, { material: "rock", mask: "terrain/rock.png" }];
    const full = await encodeMaskPng({ width: 8, height: 6, pixels: new Uint8Array(48).fill(255) });
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText(covers), "terrain/rock.png": full });

    const result = await runStrokes(project.reader, NO_LIFT, toJson([{ ...PAINT_ROCK, points: [[1, 1]] }]));

    expect(result).toEqual({ status: "ok", message: "Мазки выполнены, изменений нет: ничего не записано", writes: [] });
  });

  it("печатает по-русски, какие файлы записала", async () => {
    const project = projectOf({ "game.json": GAME_JSON, "terrain.json": terrainText() });

    const result = await runStrokes(project.reader, NO_LIFT, toJson([PAINT_ROCK, RAISE]));

    expect(result.status === "ok" && result.message).toBe("Записано: terrain/rock.png, terrain.json");
  });
});

describe("runStrokes — проект без файла рельефа", () => {
  it("первый мазок кисти рельефа создаёт terrain.json и дописывает files.terrain: ровная земля, без воды и покрытий", async () => {
    const project = projectOf({ "game.json": GAME_JSON_WITHOUT_TERRAIN });

    const writes = writtenBy(await runStrokes(project.reader, NO_LIFT, toJson([RAISE])));

    expect(writes.map((write) => write.path)).toEqual(["terrain.json", "game.json"]);
    const created = parseTerrainText(textWritten(writes, "terrain.json"));
    expect(created?.heights[3 * 9 + 4]).toBe(1);
    expect(created?.water).toBeNull();
    expect(created?.covers).toBeNull();
    expect(parseProjectFilePaths(textWritten(writes, "game.json"))?.terrain).toBe("terrain.json");
  });

  it("первый мазок «Покрасить» кладёт материал первым слоем на весь рельеф, без маски", async () => {
    const project = projectOf({ "game.json": GAME_JSON_WITHOUT_TERRAIN });

    const writes = writtenBy(await runStrokes(project.reader, NO_LIFT, toJson([{ ...PAINT_ROCK, material: "grass" }])));

    expect(writes.map((write) => write.path)).toEqual(["terrain.json", "game.json"]);
    expect(parseTerrainText(textWritten(writes, "terrain.json"))?.covers).toEqual([{ material: "grass" }]);
  });

  it("имя занято чужим файлом — terrain-2.json", async () => {
    const project = projectOf({ "game.json": GAME_JSON_WITHOUT_TERRAIN, "terrain.json": "чужое" });

    const writes = writtenBy(await runStrokes(project.reader, NO_LIFT, toJson([RAISE])));

    expect(writes.map((write) => write.path)).toEqual(["terrain-2.json", "game.json"]);
    expect(parseProjectFilePaths(textWritten(writes, "game.json"))?.terrain).toBe("terrain-2.json");
  });

  it("мазок без изменений файлов не создаёт", async () => {
    const project = projectOf({ "game.json": GAME_JSON_WITHOUT_TERRAIN });

    const result = await runStrokes(project.reader, NO_LIFT, toJson([{ ...RAISE, shift: true, brush: "smooth" }]));

    expect(result.status === "ok" && result.writes).toEqual([]);
  });
});
