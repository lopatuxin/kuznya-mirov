import { describe, expect, it } from "vitest";
import type { MaskSet } from "./maskBytes";
import { createPaintSceneController, type PaintContext, type PaintPointer, type PaintSceneContext, type PaintSceneEngine } from "./paintSceneController";
import type { PaintResult } from "./paintStroke";
import type { TerrainCoverLayer } from "./terrainFile";

const SCENE = { width: 4, height: 3 };
const GRASS_ROCK: TerrainCoverLayer[] = [{ material: "grass" }, { material: "rock", mask: "terrain/rock.png" }];
const BLANK_MASKS: MaskSet = { "terrain/rock.png": { width: 8, height: 6, pixels: new Uint8Array(48) } };

/** Сцена 4 × 3 клетки, десять точек экрана на клетку: точка экрана `(x, y)` — место сцены `(x / 10, y / 10)`. */
function pointer(x: number, y: number, extra: Partial<PaintPointer> = {}): PaintPointer {
  return { pointerId: 1, button: 0, buttons: 1, x, y, shiftKey: false, timeStamp: 0, ...extra };
}

function setup(overrides: Partial<PaintContext> = {}) {
  const coverSets: { covers: TerrainCoverLayer[]; masks: { width: number; height: number; pixels: Uint8Array }[] }[] = [];
  const calls: string[] = [];
  const terrainSets: { heights: Float64Array; water: unknown; stamps: unknown }[] = [];
  const commits: PaintResult[] = [];
  const activity: boolean[] = [];
  const restores: string[] = [];
  const state = { isSky: false, coversError: undefined as string | undefined };
  const engine = {
    terrain_at: (x: number, y: number) => (state.isSky ? undefined : [x / 10, y / 10, 0]),
    terrain_heights: () => ({ density: 2, columns: 9, rows: 7, heights: new Float64Array(63), effective: new Float64Array(63), water: null }),
    set_terrain: (heights: Float64Array, water: unknown, stamps: unknown) => {
      calls.push("set_terrain");
      terrainSets.push({ heights: Float64Array.from(heights), water, stamps });
      return undefined;
    },
    set_covers: (covers: TerrainCoverLayer[], masks: { width: number; height: number; pixels: Uint8Array }[]) => {
      calls.push("set_covers");
      coverSets.push({ covers: structuredClone(covers), masks: masks.map((mask) => ({ ...mask, pixels: Uint8Array.from(mask.pixels) })) });
      return state.coversError;
    },
  } as unknown as PaintSceneEngine;
  const paint: PaintContext = {
    material: "rock",
    size: 4,
    strength: 50,
    covers: GRASS_ROCK,
    masks: BLANK_MASKS,
    sceneSize: SCENE,
    hasTerrainFile: true,
    tintPath: null,
    onCommit: (result) => commits.push(result),
    onRestore: () => restores.push("restore"),
    ...overrides,
  };
  const context: PaintSceneContext = { engine, paint, onStrokeActiveChange: (isActive) => activity.push(isActive) };
  const controller = createPaintSceneController();
  return { controller, context, calls, coverSets, terrainSets, commits, activity, restores, state };
}

describe("мазок «Покрасить»", () => {
  it("нажатие на землю начинает мазок и говорит об этом странице", () => {
    const scene = setup();

    expect(scene.controller.start(scene.context, pointer(20, 15))).toBe(true);

    expect(scene.controller.isActive()).toBe(true);
    expect(scene.activity).toEqual([true]);
  });

  it("указатель мимо земли (небо) — мазка нет", () => {
    const scene = setup();
    scene.state.isSky = true;

    expect(scene.controller.start(scene.context, pointer(20, 15))).toBe(false);

    expect(scene.activity).toEqual([]);
  });

  it("слой с маской, которой нет, — мазка нет", () => {
    const scene = setup({ masks: {} });

    expect(scene.controller.start(scene.context, pointer(20, 15))).toBe(false);
  });

  it("каждый кадр движок получает слои и маски — сцена сразу показывает покраску", () => {
    const scene = setup();
    scene.controller.start(scene.context, pointer(20, 15));

    scene.controller.frame(scene.context, 16);
    scene.controller.frame(scene.context, 32);

    expect(scene.coverSets).toHaveLength(2);
    expect(scene.coverSets[0]?.covers).toEqual(GRASS_ROCK);
    expect(scene.coverSets[0]?.masks).toHaveLength(1);
    expect(scene.coverSets[0]?.masks[0]?.pixels.some((value) => value > 0)).toBe(true);
    const first = scene.coverSets[0]?.masks[0]?.pixels.reduce((sum, value) => sum + value, 0) as number;
    const second = scene.coverSets[1]?.masks[0]?.pixels.reduce((sum, value) => sum + value, 0) as number;
    expect(second).toBeGreaterThan(first);
  });

  it("новый слой уходит движку сразу с первым кадром: слои и маски — по порядку", () => {
    const scene = setup({ material: "scree" });
    scene.controller.start(scene.context, pointer(20, 15));

    scene.controller.frame(scene.context, 16);

    expect(scene.coverSets[0]?.covers).toEqual([...GRASS_ROCK, { material: "scree", mask: "terrain/scree.png" }]);
    expect(scene.coverSets[0]?.masks.map((mask) => [mask.width, mask.height])).toEqual([[8, 6], [16, 12]]);
  });

  it("кадр без времени и кадр с указателем мимо земли ничего не шлют", () => {
    const scene = setup();
    scene.controller.start(scene.context, pointer(20, 15));

    scene.controller.frame(scene.context, 0);
    scene.state.isSky = true;
    scene.controller.frame(scene.context, 16);

    expect(scene.coverSets).toEqual([]);
  });

  it("отпускание — одно действие: слои и изменившиеся маски", () => {
    const scene = setup();
    scene.controller.start(scene.context, pointer(20, 15));
    scene.controller.frame(scene.context, 16);

    expect(scene.controller.pointerUp(pointer(20, 15, { buttons: 0 }))).toBe(true);

    expect(scene.commits).toHaveLength(1);
    expect(scene.commits[0]?.covers).toEqual(GRASS_ROCK);
    expect(Object.keys(scene.commits[0]?.masks ?? {})).toEqual(["terrain/rock.png"]);
    expect(scene.activity).toEqual([true, false]);
    expect(scene.controller.isActive()).toBe(false);
  });

  it("отпускание другой кнопки, пока левая нажата, мазок не кончает", () => {
    const scene = setup();
    scene.controller.start(scene.context, pointer(20, 15));

    expect(scene.controller.pointerUp(pointer(20, 15, { buttons: 1 }))).toBe(false);
    expect(scene.controller.pointerUp(pointer(20, 15, { pointerId: 2, buttons: 0 }))).toBe(false);

    expect(scene.controller.isActive()).toBe(true);
  });

  it("мазок без изменений — не действие", () => {
    const scene = setup({ masks: { "terrain/rock.png": { width: 8, height: 6, pixels: new Uint8Array(48).fill(255) } } });
    scene.controller.start(scene.context, pointer(20, 15));
    scene.controller.frame(scene.context, 16);

    scene.controller.pointerUp(pointer(20, 15, { buttons: 0 }));

    expect(scene.commits).toEqual([]);
    expect(scene.activity).toEqual([true, false]);
  });

  it("Shift смотрится в каждом кадре: двинул указатель с Shift — кисть стирает", () => {
    const scene = setup({ masks: { "terrain/rock.png": { width: 8, height: 6, pixels: new Uint8Array(48).fill(255) } } });
    scene.controller.start(scene.context, pointer(20, 15));
    scene.controller.frame(scene.context, 16);
    expect(scene.coverSets[0]?.masks[0]?.pixels.every((value) => value === 255)).toBe(true);

    scene.controller.pointerMove(pointer(20, 15, { shiftKey: true }));
    scene.controller.frame(scene.context, 32);

    expect(scene.coverSets[1]?.masks[0]?.pixels.some((value) => value < 255)).toBe(true);
  });

  it("клавиша Shift посреди мазка работает так же, как Shift у указателя", () => {
    const scene = setup({ masks: { "terrain/rock.png": { width: 8, height: 6, pixels: new Uint8Array(48).fill(255) } } });
    scene.controller.start(scene.context, pointer(20, 15));

    scene.controller.noteShift(true);
    scene.controller.frame(scene.context, 16);

    expect(scene.coverSets[0]?.masks[0]?.pixels.some((value) => value < 255)).toBe(true);
  });

  it("мазок с Shift по слою, которого нет среди слоёв, ничего не делает", () => {
    const scene = setup({ material: "scree" });
    scene.controller.start(scene.context, pointer(20, 15, { shiftKey: true }));

    scene.controller.frame(scene.context, 16);
    scene.controller.pointerUp(pointer(20, 15, { buttons: 0 }));

    expect(scene.coverSets).toEqual([]);
    expect(scene.commits).toEqual([]);
  });

  it("круг кисти идущего мазка — поперечник и место указателя", () => {
    const scene = setup({ size: 7 });
    expect(scene.controller.circle()).toBeNull();

    scene.controller.start(scene.context, pointer(20, 15));
    scene.controller.pointerMove(pointer(30, 25));

    expect(scene.controller.circle()).toEqual({ size: 7, pointer: [30, 25] });
  });
});

describe("отмена мазка", () => {
  it("Esc возвращает движку слои и маски до мазка, файлы не пишутся", () => {
    const scene = setup({ material: "scree" });
    scene.controller.start(scene.context, pointer(20, 15));
    scene.controller.frame(scene.context, 16);

    expect(scene.controller.cancel(scene.context)).toBe(true);

    expect(scene.coverSets.at(-1)?.covers).toEqual(GRASS_ROCK);
    expect(scene.coverSets.at(-1)?.masks).toEqual([{ width: 8, height: 6, pixels: new Uint8Array(48) }]);
    expect(scene.commits).toEqual([]);
    expect(scene.activity).toEqual([true, false]);
    expect(scene.controller.isActive()).toBe(false);
  });

  it("Esc без мазка ничего не делает", () => {
    const scene = setup();

    expect(scene.controller.cancel(scene.context)).toBe(false);
  });

  it("сброс указателя — как Esc", () => {
    const scene = setup();
    scene.controller.start(scene.context, pointer(20, 15));
    scene.controller.frame(scene.context, 16);

    expect(scene.controller.pointerCancel(scene.context, pointer(20, 15, { buttons: 0 }))).toBe(true);

    expect(scene.coverSets.at(-1)?.masks[0]?.pixels.every((value) => value === 0)).toBe(true);
    expect(scene.commits).toEqual([]);
  });

  it("мазок, ничего не изменивший, не трогает движок при отмене", () => {
    const scene = setup();
    scene.controller.start(scene.context, pointer(20, 15));

    scene.controller.cancel(scene.context);

    expect(scene.coverSets).toEqual([]);
    expect(scene.restores).toEqual([]);
  });

  it("в проекте без покрытий движок вызовом к ним не вернуть — страница собирает мир из файлов заново", () => {
    const scene = setup({ covers: null, masks: {}, material: "grass" });
    scene.controller.start(scene.context, pointer(20, 15));
    scene.controller.frame(scene.context, 16);

    scene.controller.cancel(scene.context);

    expect(scene.restores).toEqual(["restore"]);
    expect(scene.activity).toEqual([true, false]);
  });

  it("движок отказал — мазок бросается без действия", () => {
    const scene = setup();
    scene.controller.start(scene.context, pointer(20, 15));
    scene.state.coversError = "идёт партия";

    scene.controller.frame(scene.context, 16);

    expect(scene.controller.isActive()).toBe(false);
    expect(scene.activity).toEqual([true, false]);
    expect(scene.controller.pointerUp(pointer(20, 15, { buttons: 0 }))).toBe(false);
    expect(scene.commits).toEqual([]);
  });

  it("мир собран заново — мазок бросается без возврата слоёв", () => {
    const scene = setup();
    scene.controller.start(scene.context, pointer(20, 15));
    scene.controller.frame(scene.context, 16);
    const sent = scene.coverSets.length;

    scene.controller.abandon();

    expect(scene.coverSets).toHaveLength(sent);
    expect(scene.activity).toEqual([true, false]);
    expect(scene.controller.isActive()).toBe(false);
  });
});

describe("проект без файла рельефа", () => {
  it("ровная земля ставится движку первым кадром, что что-то меняет, и до покрытий — иначе движок их не примет", () => {
    const scene = setup({ hasTerrainFile: false, covers: null, masks: {}, material: "grass" });

    scene.controller.start(scene.context, pointer(20, 15));
    expect(scene.terrainSets).toEqual([]);
    scene.controller.frame(scene.context, 16);
    scene.controller.frame(scene.context, 32);

    expect(scene.calls).toEqual(["set_terrain", "set_covers", "set_covers"]);
    expect(scene.terrainSets).toHaveLength(1);
    expect(scene.terrainSets[0]?.heights.length).toBe(63);
    expect(scene.terrainSets[0]?.water).toBeNull();
    expect(scene.terrainSets[0]?.stamps).toEqual([]);
  });

  it("мазок с Shift, нажатие без кадров и Esc до первого кадра не трогают движок проекта без файла", () => {
    const options = { hasTerrainFile: false, covers: null, masks: {}, material: "grass" };
    const shifted = setup(options);
    shifted.controller.start(shifted.context, pointer(20, 15, { shiftKey: true }));
    shifted.controller.frame(shifted.context, 16);
    shifted.controller.pointerUp(pointer(20, 15, { buttons: 0 }));
    const clicked = setup(options);
    clicked.controller.start(clicked.context, pointer(20, 15));
    clicked.controller.pointerUp(pointer(20, 15, { buttons: 0 }));
    const cancelled = setup(options);
    cancelled.controller.start(cancelled.context, pointer(20, 15));
    cancelled.controller.cancel(cancelled.context);

    expect([shifted.calls, clicked.calls, cancelled.calls]).toEqual([[], [], []]);
    expect([shifted.commits, clicked.commits, cancelled.commits]).toEqual([[], [], []]);
    expect(cancelled.restores).toEqual([]);
  });

  it("у проекта с файлом рельеф движку заново не ставится", () => {
    const scene = setup();

    scene.controller.start(scene.context, pointer(20, 15));
    scene.controller.frame(scene.context, 16);

    expect(scene.terrainSets).toEqual([]);
  });

  it("первый мазок кладёт материал первым слоем, отпускание отдаёт его странице", () => {
    const scene = setup({ hasTerrainFile: false, covers: null, masks: {}, material: "grass" });
    scene.controller.start(scene.context, pointer(20, 15));
    scene.controller.frame(scene.context, 16);

    scene.controller.pointerUp(pointer(20, 15, { buttons: 0 }));

    expect(scene.coverSets[0]?.covers).toEqual([{ material: "grass" }]);
    expect(scene.commits[0]).toEqual({ covers: [{ material: "grass" }], masks: {} });
  });
});
