import { describe, expect, it, vi } from "vitest";
import { createEditorCameraStore, type EditorCameraRequest, type EditorCameraState } from "./editorCamera";
import type { ImprintContext } from "./imprintSceneController";
import type { PaintContext } from "./paintSceneController";
import type { PaintResult } from "./paintStroke";
import type { PlacementChange } from "./objectPlacement";
import {
  createSpaceSceneController,
  type KeyInput,
  type PointerInput,
  type SpaceSceneContext,
  type SpaceSceneEngine,
} from "./spaceSceneController";
import type { BrushGrid, BrushSettings } from "./terrainBrush";

const START_CAMERA: EditorCameraState = { target: [16, 12, 0], yaw: 0, pitch: 55, distance: 40 };
const NO_KEYS: KeyInput = { code: "", ctrlKey: false, altKey: false, metaKey: false, shiftKey: false };

/** Камера сверху без перспективы: 10 точек на клетку, начало сцены в (50, 50). */
function ground(x: number, y: number): [number, number] {
  return [(x - 50) / 10, (y - 50) / 10];
}

function pointer(x: number, y: number, extra: Partial<PointerInput> = {}): PointerInput {
  return { pointerId: 1, button: 0, buttons: 1, x, y, ctrlKey: false, shiftKey: false, timeStamp: 0, ...extra };
}

function release(x: number, y: number, button = 0): PointerInput {
  return pointer(x, y, { button, buttons: 0 });
}

function key(code: string, extra: Partial<KeyInput> = {}): KeyInput {
  return { ...NO_KEYS, code, ...extra };
}

function setup(overrides: Partial<SpaceSceneContext> = {}, objects: Record<number, Record<string, unknown>> = {}) {
  const worldObjects: Record<number, Record<string, unknown>> = {
    7: { position: [3, 4], size: [2, 2], height: 2, shape: "box", color: "#8a8f94" },
    ...objects,
  };
  const moves: [number, number, number, number | null | undefined][] = [];
  const restCalls: [number, number, number, number | null | undefined][] = [];
  const transforms: [number, Record<string, unknown>][] = [];
  const cameras: EditorCameraRequest[] = [];
  const terrainSets: { heights: Float64Array; water: unknown; stamps: unknown }[] = [];
  const fitCalls: unknown[] = [];
  const coverSets: { covers: unknown; masks: unknown }[] = [];
  const state = {
    pickedId: undefined as number | undefined,
    fitted: START_CAMERA as EditorCameraState | undefined,
    /** Высота земли под указателем. */
    groundZ: 0,
    /** Высота, на которую движок посадил бы объект: `from` `undefined` — как без `z` в данных. */
    rest: (_id: number, _x: number, _y: number, _from: number | null | undefined): number => 0,
    /** Основание, на котором объект стоит в мире после `transform_object`; без значения — третье число `position`. */
    worldHeight: undefined as number | undefined,
    /** Луч мимо рельефа сцены (небо) — `terrain_at` ничего не отдаёт. */
    isSkyUnderPointer: false,
    /** Рельеф сцены 12 × 12 клеток: точек 25 × 25, все на нуле. */
    grid: { density: 2, columns: 25, rows: 25, heights: new Float64Array(25 * 25) } as BrushGrid,
    water: null as unknown,
    setTerrainError: undefined as string | undefined,
    /** Итоговые высоты с отпечатками; без значения они равны высотам файла. */
    effective: undefined as Float64Array | undefined,
    /** Номер отпечатка под указателем. */
    pickedStamp: undefined as number | undefined,
  };
  const engine = {
    object_at: () => state.pickedId,
    object_rect: () => ({ corners: [[10, 10], [20, 10], [20, 20], [10, 20]] }),
    object_properties: (id: number) => {
      const properties = worldObjects[id];
      const position = properties?.position as number[] | undefined;
      return properties === undefined || position === undefined ? undefined : { ...properties, position: [position[0], position[1], state.worldHeight ?? position[2] ?? 0] };
    },
    ground_at: (x: number, y: number) => [...ground(x, y), state.groundZ],
    rest_height: (id: number, x: number, y: number, from?: number | null) => {
      restCalls.push([id, x, y, from]);
      return state.rest(id, x, y, from);
    },
    screen_point: (x: number, y: number, z: number) => [x * 10 + 50, y * 10 + 50 - z * 10],
    move_object: (id: number, x: number, y: number, z?: number | null) => moves.push([id, x, y, z]),
    transform_object: (id: number, transform: Record<string, unknown>) => transforms.push([id, transform]),
    editor_camera: (camera: EditorCameraRequest) => {
      cameras.push(camera);
      return camera.target[2] ?? 0;
    },
    terrain_at: (x: number, y: number) => (state.isSkyUnderPointer ? undefined : [...ground(x, y), state.groundZ]),
    terrain_height: () => 0,
    terrain_heights: () => ({ ...state.grid, heights: Float64Array.from(state.grid.heights), effective: Float64Array.from(state.effective ?? state.grid.heights), water: state.water }),
    set_terrain: (heights: Float64Array, water: unknown, stamps: unknown) => {
      terrainSets.push({ heights: Float64Array.from(heights), water, stamps });
      return state.setTerrainError;
    },
    set_covers: (covers: unknown, masks: unknown) => {
      coverSets.push({ covers, masks });
      return undefined;
    },
    stamp_at: () => state.pickedStamp,
    fit_camera: (id?: number | null) => {
      fitCalls.push(id);
      return state.fitted;
    },
  } as unknown as SpaceSceneEngine;

  const cameraStore = createEditorCameraStore();
  cameraStore.restore(engine);
  cameras.length = 0;
  fitCalls.length = 0;

  const commits: [number, PlacementChange[]][] = [];
  const selections: (number | null)[] = [];
  const modes: string[] = [];
  const terrainCommits: Float64Array[] = [];
  const strokeStates: boolean[] = [];
  const imprintSelections: number[] = [];
  const imprintPlaced: unknown[] = [];
  const imprints: ImprintContext = {
    ...setupImprints(),
    onSelect: (index) => imprintSelections.push(index),
    onPlace: (entry) => imprintPlaced.push(entry),
  };
  const context: SpaceSceneContext = {
    engine,
    cameraStore,
    isInputLocked: false,
    isEditorCameraActive: true,
    areHandlesAvailable: true,
    selectedIndex: 7,
    selectedLabel: "izba",
    handleMode: "translate",
    brush: null,
    paint: null,
    getObjectProperties: (id) => worldObjects[id] ?? null,
    onSelect: (index) => selections.push(index),
    onHandleModeChange: (mode) => modes.push(mode),
    onCommitPlacement: (index, changes) => commits.push([index, changes]),
    onCommitTerrain: (grid) => terrainCommits.push(Float64Array.from(grid.heights)),
    onStrokeActiveChange: (isActive) => strokeStates.push(isActive),
    imprints,
    ...overrides,
  };
  const controller = createSpaceSceneController(() => context);
  return { controller, context, moves, restCalls, transforms, cameras, fitCalls, commits, selections, modes, state, cameraStore, terrainSets, coverSets, terrainCommits, strokeStates, imprintSelections, imprintPlaced, engine };
}

describe("выбор щелчком", () => {
  it("щелчок выбирает объект, что назвал движок, а мимо — снимает выбор", () => {
    const scene = setup({ selectedIndex: null });
    scene.state.pickedId = 7;
    scene.controller.pointerDown(pointer(100, 100));
    scene.controller.pointerUp(release(100, 100));
    scene.state.pickedId = undefined;
    scene.controller.pointerDown(pointer(400, 400));
    expect(scene.selections).toEqual([7, null]);
    expect(scene.commits).toEqual([]);
  });

  it("ручка выбранного объекта важнее объекта под ней", () => {
    const scene = setup();
    scene.state.pickedId = 9;
    expect(scene.controller.pointerDown(pointer(150, 100))).toBe(true);
    expect(scene.selections).toEqual([]);
  });

  it("в идущей партии мышь принадлежит игре: ни выбора, ни жеста", () => {
    const scene = setup({ isInputLocked: true });
    scene.state.pickedId = 7;
    expect(scene.controller.pointerDown(pointer(100, 100))).toBe(false);
    expect(scene.controller.pointerDown(pointer(100, 100, { button: 1, buttons: 4 }))).toBe(false);
    expect(scene.selections).toEqual([]);
  });

  it("в повторе объект выбирается, но ручек и переноса нет", () => {
    const scene = setup({ areHandlesAvailable: false, selectedIndex: null });
    scene.state.pickedId = 7;
    expect(scene.controller.pointerDown(pointer(100, 100))).toBe(false);
    expect(scene.selections).toEqual([7]);
  });
});

describe("отпечатки при щелчке", () => {
  const BELUHA = { stamp: "beluha", position: [10, 10], size: [8, 8], height: 4 };

  it("объект под указателем важнее отпечатка под ним: выбирается объект, отпечаток — нет", () => {
    const scene = setup({ selectedIndex: null });
    scene.state.pickedId = 7;
    scene.state.pickedStamp = 0;
    scene.controller.pointerDown(pointer(100, 100));
    expect(scene.selections).toEqual([7]);
    expect(scene.imprintSelections).toEqual([]);
  });

  it("объекта нет, отпечаток есть — выбирается отпечаток, объектов выбор не снимает вызовом onSelect(null)", () => {
    const scene = setup({ selectedIndex: null });
    scene.state.pickedStamp = 2;
    scene.controller.pointerDown(pointer(100, 100));
    expect(scene.imprintSelections).toEqual([2]);
    expect(scene.selections).toEqual([]);
  });

  it("нет ни объекта, ни отпечатка — выбор снимается", () => {
    const scene = setup({ selectedIndex: null });
    expect(scene.controller.pointerDown(pointer(100, 100))).toBe(false);
    expect(scene.selections).toEqual([null]);
    expect(scene.imprintSelections).toEqual([]);
  });

  it("в повторе и на паузе отпечатки не выбираются", () => {
    const scene = setup({ selectedIndex: null, imprints: { ...setupImprints(), isEditable: false } });
    scene.state.pickedStamp = 2;
    scene.controller.pointerDown(pointer(100, 100));
    expect(scene.imprintSelections).toEqual([]);
    expect(scene.selections).toEqual([null]);
  });

  it("при кнопке «Отпечаток» щелчок ставит отпечаток и ничего не выбирает; Esc возвращает ручки", () => {
    const placing = { stamp: { name: "beluha", columns: 4, rows: 2 }, width: 8, height: 4 };
    const scene = setup({ imprints: { ...setupImprints(), placing, onPlace: (entry) => scene.imprintPlaced.push(entry) } });
    scene.state.pickedId = 7;
    expect(scene.controller.pointerDown(pointer(100, 100))).toBe(false);
    expect(scene.imprintPlaced).toEqual([{ stamp: "beluha", position: [5, 5], size: [8, 4], height: 4 }]);
    expect(scene.selections).toEqual([]);
    expect(scene.controller.keyDown(key("Escape"))).toBe(true);
    expect(scene.modes).toEqual(["translate"]);
  });

  it("щелчок мимо земли при «Отпечатке» отпечатка не ставит", () => {
    const placing = { stamp: { name: "beluha", columns: 4, rows: 2 }, width: 8, height: 4 };
    const scene = setup({ imprints: { ...setupImprints(), placing, onPlace: (entry) => scene.imprintPlaced.push(entry) } });
    scene.state.isSkyUnderPointer = true;
    scene.controller.pointerDown(pointer(100, 100));
    expect(scene.imprintPlaced).toEqual([]);
  });

  it("рамка выбранного отпечатка рисуется вместе с рамкой объекта", () => {
    const scene = setup({ selectedIndex: null, imprints: { ...setupImprints(), entries: [BELUHA], selectedIndex: 0 } });
    const terrainHeight = vi.spyOn(scene.engine, "terrain_height");
    scene.controller.draw(recordingCanvasContext(), 1);
    expect(terrainHeight.mock.calls.length).toBeGreaterThanOrEqual(32);
  });
});

describe("перенос ручкой", () => {
  it("стрелка оси меняет одну координату, а при отпускании — одно действие с прежним значением", () => {
    const scene = setup();
    expect(scene.controller.pointerDown(pointer(150, 100))).toBe(true);
    scene.controller.pointerMove(pointer(170, 120));
    expect(scene.moves).toEqual([[7, 5, 4, 0]]);
    scene.controller.pointerUp(release(170, 120));
    expect(scene.commits).toEqual([[7, [{ key: "position", value: [5, 4], previous: [3, 4] }]]]);
  });

  it("середина двигает объект свободно до сотой клетки, с Ctrl — до целой", () => {
    const free = setup();
    free.controller.pointerDown(pointer(92, 102));
    free.controller.pointerMove(pointer(95, 113));
    expect(free.moves.at(-1)).toEqual([7, 3.3, 5.1, 0]);

    const snapped = setup();
    snapped.controller.pointerDown(pointer(92, 102));
    snapped.controller.pointerMove(pointer(95, 120, { ctrlKey: true }));
    expect(snapped.moves.at(-1)).toEqual([7, 3, 6, 0]);
  });

  it("Esc отменяет перенос: объект возвращается как был, действия нет", () => {
    const scene = setup();
    scene.controller.pointerDown(pointer(150, 100));
    scene.controller.pointerMove(pointer(170, 120));
    expect(scene.controller.keyDown(key("Escape"))).toBe(true);
    expect(scene.moves.at(-1)).toEqual([7, 3, 4, 0]);
    scene.controller.pointerUp(release(170, 120));
    expect(scene.commits).toEqual([]);
  });

  it("отпускание без движения — не действие", () => {
    const scene = setup();
    scene.controller.pointerDown(pointer(150, 100));
    scene.controller.pointerUp(release(150, 100));
    expect(scene.commits).toEqual([]);
  });
});

describe("перенос самого объекта", () => {
  it("сдвиг дальше 4 точек начинает перенос, объект идёт за точкой земли под указателем", () => {
    const scene = setup({ selectedIndex: null });
    scene.state.pickedId = 7;
    expect(scene.controller.pointerDown(pointer(100, 100))).toBe(true);
    scene.controller.pointerMove(pointer(103, 102));
    expect(scene.moves).toEqual([]);
    scene.controller.pointerMove(pointer(120, 100));
    expect(scene.moves).toEqual([[7, 5, 4, 0]]);
    scene.controller.pointerUp(release(120, 100));
    expect(scene.commits).toEqual([[7, [{ key: "position", value: [5, 4], previous: [3, 4] }]]]);
  });

  it("в любом виде ручек нажатие по объекту вне ручек переносит его, а не поворачивает", () => {
    const scene = setup({ handleMode: "rotate" });
    scene.state.pickedId = 7;
    scene.controller.pointerDown(pointer(120, 100));
    scene.controller.pointerMove(pointer(140, 100));
    expect(scene.moves).toEqual([[7, 5, 4, 0]]);
    expect(scene.transforms).toEqual([]);
  });

  it("точка земли под указателем над горизонтом — объект не двигается", () => {
    const scene = setup({ selectedIndex: null });
    scene.state.pickedId = 7;
    scene.controller.pointerDown(pointer(100, 100));
    (scene.context.engine as unknown as { ground_at: () => undefined }).ground_at = () => undefined;
    scene.controller.pointerMove(pointer(150, 100));
    expect(scene.moves).toEqual([]);
  });
});

describe("поворот ручкой", () => {
  it("−90 и 10° по часовой — пишется 280, место и размер не меняются", () => {
    const scene = setup({ handleMode: "rotate" }, { 7: { position: [3, 4], size: [2, 2], height: 2, rotation: -90, shape: "box" } });
    expect(scene.controller.pointerDown(pointer(170, 100))).toBe(true);
    const radians = (10 * Math.PI) / 180;
    scene.controller.pointerMove(pointer(90 + 80 * Math.cos(radians), 100 + 80 * Math.sin(radians)));
    expect(scene.transforms.at(-1)).toEqual([7, { position: [3, 4], size: [2, 2], rotation: 280 }]);
    scene.controller.pointerUp(release(170, 108));
    expect(scene.commits).toEqual([[7, [{ key: "rotation", value: 280, previous: -90 }]]]);
  });

  it("с Ctrl — кратно 15°", () => {
    const scene = setup({ handleMode: "rotate" });
    scene.controller.pointerDown(pointer(170, 100));
    const radians = (40 * Math.PI) / 180;
    scene.controller.pointerMove(pointer(90 + 80 * Math.cos(radians), 100 + 80 * Math.sin(radians), { ctrlKey: true }));
    scene.controller.pointerUp(release(170, 150));
    expect(scene.commits[0]?.[1]).toEqual([{ key: "rotation", value: 45, previous: undefined }]);
  });
});

describe("масштаб ручкой", () => {
  it("ось вдоль стороны повёрнутого объекта: середина на месте, меняется size и position", () => {
    const scene = setup({ handleMode: "scale" }, { 7: { position: [3, 4], size: [2, 2], height: 2, rotation: 90, shape: "box" } });
    expect(scene.controller.pointerDown(pointer(90, 190))).toBe(true);
    scene.controller.pointerMove(pointer(90, 145));
    expect(scene.transforms.at(-1)).toEqual([7, { position: [3.5, 4], size: [1, 2] }]);
    scene.controller.pointerUp(release(90, 145));
    expect(scene.commits[0]?.[1]).toEqual([
      { key: "position", value: [3.5, 4], previous: [3, 4] },
      { key: "size", value: [1, 2], previous: [2, 2] },
    ]);
  });

  it("общая ручка меняет всё в одной доле и пишет высоту", () => {
    const scene = setup({ handleMode: "scale" });
    scene.controller.pointerDown(pointer(90, 100));
    scene.controller.pointerMove(pointer(135, 55));
    scene.controller.pointerUp(release(135, 55));
    const keys = scene.commits[0]?.[1].map((change) => change.key);
    expect(keys).toEqual(["position", "size", "height"]);
    const size = scene.commits[0]?.[1].find((change) => change.key === "size")?.value as number[];
    expect(size[0]).toBeGreaterThan(2);
    expect(size[0]).toBe(size[1]);
  });

  it("ручка высоты меняет только height, у фигуры без height — от единицы", () => {
    const scene = setup({ handleMode: "scale" }, { 7: { position: [3, 4], size: [2, 2], shape: "box" } });
    expect(scene.controller.pointerDown(pointer(90, 10))).toBe(true);
    scene.controller.pointerMove(pointer(90, -35));
    scene.controller.pointerUp(release(90, -35));
    expect(scene.commits[0]?.[1]).toEqual([{ key: "height", value: 1.5, previous: undefined }]);
  });

  it("у плоского объекта на земле ручки высоты нет", () => {
    const scene = setup({ handleMode: "scale" }, { 7: { position: [3, 4], size: [2, 2], image: "grass" } });
    scene.state.pickedId = undefined;
    expect(scene.controller.pointerDown(pointer(90, 10))).toBe(false);
    expect(scene.selections).toEqual([null]);
  });
});

/**
 * Мост над оврагом: за `x = 6` верх моста на 0,3, под ним берег на −2, а слева плато на 0. Настил
 * встречает только объект, что стоял не ниже его верха минус 0,4.
 */
function bridgeTerrain(_id: number, x: number, _y: number, from: number | null | undefined): number {
  if (x < 6) return 0;
  return from === undefined || from === null || from + 0.4 >= 0.3 ? 0.3 : -2;
}

describe("перенос по рельефу", () => {
  it("за тело — по высоте точки под указателем: на мост — два числа, под мост — три", () => {
    const onBridge = setup({ selectedIndex: null });
    onBridge.state.rest = bridgeTerrain;
    onBridge.state.pickedId = 7;
    onBridge.controller.pointerDown(pointer(100, 100));
    onBridge.state.groundZ = 0.3;
    onBridge.controller.pointerMove(pointer(400, 100));
    expect(onBridge.moves.at(-1)).toEqual([7, 33, 4, 0.3]);
    expect(onBridge.restCalls.at(-1)).toEqual([7, 33, 4, 0.3]);
    onBridge.controller.pointerUp(release(400, 100));
    expect(onBridge.commits).toEqual([[7, [{ key: "position", value: [33, 4], previous: [3, 4] }]]]);

    const underBridge = setup({ selectedIndex: null });
    underBridge.state.rest = bridgeTerrain;
    underBridge.state.pickedId = 7;
    underBridge.controller.pointerDown(pointer(100, 100));
    underBridge.state.groundZ = -2;
    underBridge.controller.pointerMove(pointer(400, 100));
    expect(underBridge.moves.at(-1)).toEqual([7, 33, 4, -2]);
    underBridge.controller.pointerUp(release(400, 100));
    expect(underBridge.commits).toEqual([[7, [{ key: "position", value: [33, 4, -2], previous: [3, 4] }]]]);
  });

  it("стрелка x садит от высоты начала жеста, а не от точки под указателем", () => {
    const scene = setup({}, { 7: { position: [3, 4, -2], size: [2, 2], height: 2, shape: "box" } });
    scene.state.rest = bridgeTerrain;
    expect(scene.controller.pointerDown(pointer(150, 120))).toBe(true);
    scene.state.groundZ = 0.3;
    scene.controller.pointerMove(pointer(400, 120));
    expect(scene.moves.at(-1)).toEqual([7, 28, 4, -2]);
    expect(scene.restCalls.at(-1)).toEqual([7, 28, 4, -2]);
    scene.controller.pointerUp(release(400, 120));
    expect(scene.commits).toEqual([[7, [{ key: "position", value: [28, 4, -2], previous: [3, 4, -2] }]]]);
  });

  it("на паузе партии перенос пишет всегда три числа, даже когда высота совпала с посадкой", () => {
    const scene = setup({ isEditorCameraActive: false, selectedIndex: null }, { 7: { position: [3, 4, 0], size: [2, 2], height: 2, shape: "box" } });
    scene.state.pickedId = 7;
    scene.controller.pointerDown(pointer(100, 100));
    scene.controller.pointerMove(pointer(120, 100));
    scene.controller.pointerUp(release(120, 100));
    expect(scene.commits).toEqual([[7, [{ key: "position", value: [5, 4, 0], previous: [3, 4, 0] }]]]);
  });

  it("ручки стоят на высоте основания: нажатие на середину, поднятую на 2 клетки, берёт ручку", () => {
    const scene = setup({}, { 7: { position: [3, 4, 2], size: [2, 2], height: 2, shape: "box" } });
    scene.state.pickedId = undefined;
    expect(scene.controller.pointerDown(pointer(90, 80))).toBe(true);
    expect(scene.selections).toEqual([]);
    scene.controller.pointerUp(release(90, 80));
    expect(scene.controller.pointerDown(pointer(60, 110))).toBe(false);
  });

  it("без `z` в файле ручки стоят на высоте, на которую объект садится", () => {
    const scene = setup();
    scene.state.rest = () => 2;
    expect(scene.controller.pointerDown(pointer(90, 80))).toBe(true);
  });
});

describe("вертикальная стрелка", () => {
  it("поднимает основание на столько клеток, на сколько ушёл указатель, до сотой; пишет три числа", () => {
    const scene = setup();
    expect(scene.controller.pointerDown(pointer(90, 50))).toBe(true);
    scene.controller.pointerMove(pointer(90, 18.7));
    expect(scene.moves.at(-1)).toEqual([7, 3, 4, 3.13]);
    scene.controller.pointerUp(release(90, 18.7));
    expect(scene.commits).toEqual([[7, [{ key: "position", value: [3, 4, 3.13], previous: [3, 4] }]]]);
  });

  it("с Ctrl — до целой клетки", () => {
    const scene = setup();
    scene.controller.pointerDown(pointer(90, 50));
    scene.controller.pointerMove(pointer(90, 18.7, { ctrlKey: true }));
    expect(scene.moves.at(-1)).toEqual([7, 3, 4, 3]);
  });

  it("меняет только z: место и остальные свойства не в правке", () => {
    const scene = setup();
    scene.controller.pointerDown(pointer(90, 50));
    scene.controller.pointerMove(pointer(70, 30));
    scene.controller.pointerUp(release(70, 30));
    expect(scene.commits[0]?.[1].map((change) => change.key)).toEqual(["position"]);
    expect(scene.moves.every(([, x, y]) => x === 3 && y === 4)).toBe(true);
  });

  it("вернули на высоту посадки — пишутся два числа, третье убирается", () => {
    const scene = setup({}, { 7: { position: [3, 4, 2], size: [2, 2], height: 2, shape: "box" } });
    expect(scene.controller.pointerDown(pointer(90, 40))).toBe(true);
    scene.controller.pointerMove(pointer(90, 60));
    expect(scene.moves.at(-1)).toEqual([7, 3, 4, 0]);
    scene.controller.pointerUp(release(90, 60));
    expect(scene.commits).toEqual([[7, [{ key: "position", value: [3, 4], previous: [3, 4, 2] }]]]);
  });

  it("есть и у плоского объекта, и у настила", () => {
    const flat = setup({}, { 7: { position: [3, 4], size: [2, 2], image: "grass" } });
    expect(flat.controller.pointerDown(pointer(90, 50))).toBe(true);
    const deck = setup({}, { 7: { position: [3, 4], size: [2, 2], deck: true, shape: "box", height: 0.3 } });
    expect(deck.controller.pointerDown(pointer(90, 50))).toBe(true);
  });

  it("отпускание без движения и Esc не пишут действие; Esc возвращает основание ровно", () => {
    const scene = setup({}, { 7: { position: [3, 4, 2], size: [2, 2], height: 2, shape: "box" } });
    scene.controller.pointerDown(pointer(90, 40));
    scene.controller.pointerUp(release(90, 40));
    expect(scene.commits).toEqual([]);

    scene.controller.pointerDown(pointer(90, 40));
    scene.controller.pointerMove(pointer(90, 10));
    expect(scene.controller.keyDown(key("Escape"))).toBe(true);
    expect(scene.moves.at(-1)).toEqual([7, 3, 4, 2]);
    scene.controller.pointerUp(release(90, 10));
    expect(scene.commits).toEqual([]);
  });

  it("в масштабе зелёная ручка — высота фигуры, а не основание", () => {
    const scene = setup({ handleMode: "scale" });
    scene.controller.pointerDown(pointer(90, 10));
    scene.controller.pointerMove(pointer(90, -35));
    expect(scene.transforms.at(-1)?.[1]).toEqual({ position: [3, 4], size: [2, 2], height: 3 });
    expect(scene.moves).toEqual([]);
  });
});

describe("поворот и масштаб не задают высоту", () => {
  it("объект садит движок сам: position из двух чисел, а поднятое посадкой основание не пишется", () => {
    const scene = setup({ handleMode: "rotate" });
    scene.controller.pointerDown(pointer(170, 100));
    scene.state.rest = () => 0.5;
    scene.state.worldHeight = 0.5;
    scene.controller.pointerMove(pointer(90 + 80 * Math.cos(0.3), 100 + 80 * Math.sin(0.3)));
    expect(scene.transforms.at(-1)?.[1]).toMatchObject({ position: [3, 4] });
    scene.controller.pointerUp(release(170, 124));
    expect(scene.commits[0]?.[1].map((change) => change.key)).toEqual(["rotation"]);
  });

  it("Esc возвращает основание ровно: position из трёх чисел", () => {
    const scene = setup({ handleMode: "rotate" }, { 7: { position: [3, 4, 1], size: [2, 2], height: 2, shape: "box" } });
    scene.controller.pointerDown(pointer(170, 90));
    scene.controller.pointerMove(pointer(170, 130));
    scene.controller.keyDown(key("Escape"));
    expect(scene.transforms.at(-1)?.[1]).toMatchObject({ position: [3, 4, 1], rotation: 0 });
  });
});

describe("камера редактора", () => {
  it("средняя кнопка вращает: вправо — yaw уменьшается, вниз — pitch растёт", () => {
    const scene = setup();
    expect(scene.controller.pointerDown(pointer(100, 100, { button: 1, buttons: 4 }))).toBe(true);
    scene.controller.pointerMove(pointer(150, 100, { button: -1, buttons: 4 }));
    scene.controller.pointerMove(pointer(150, 125, { button: -1, buttons: 4 }));
    scene.controller.pointerUp(release(150, 125, 1));
    expect(scene.cameras).toHaveLength(2);
    expect(scene.cameras[0]?.yaw).toBeCloseTo(340);
    expect(scene.cameras[1]?.pitch).toBeCloseTo(65);
    expect(scene.cameraStore.current()?.camera.yaw).toBeCloseTo(340);
  });

  it("Shift + средняя кнопка сдвигает точку камеры так, что взятое место идёт за указателем", () => {
    const scene = setup();
    scene.controller.pointerDown(pointer(100, 100, { button: 1, buttons: 4, shiftKey: true }));
    scene.controller.pointerMove(pointer(120, 100, { button: -1, buttons: 4, shiftKey: true }));
    expect(scene.cameras.at(-1)?.target).toEqual([14, 12]);
  });

  it("левая кнопка, отпущенная при зажатой средней, вращение не заканчивает", () => {
    const scene = setup();
    scene.controller.pointerDown(pointer(100, 100, { button: 1, buttons: 4 }));
    scene.controller.pointerUp(pointer(100, 100, { button: 0, buttons: 4 }));
    scene.controller.pointerMove(pointer(150, 100, { button: -1, buttons: 4 }));
    expect(scene.cameras).toHaveLength(1);
  });

  it("на паузе и в идущей партии средняя кнопка ничего не делает", () => {
    const paused = setup({ isEditorCameraActive: false });
    expect(paused.controller.pointerDown(pointer(100, 100, { button: 1, buttons: 4 }))).toBe(false);
    expect(paused.controller.wheel({ deltaY: -100, deltaMode: 0 })).toBe(false);
    expect(paused.cameras).toEqual([]);
  });

  it("колесо приближает: щелчок — расстояние × 0,9, но не ближе 2 клеток и не дальше трёх расстояний", () => {
    const scene = setup();
    expect(scene.controller.wheel({ deltaY: -100, deltaMode: 0 })).toBe(true);
    expect(scene.cameras[0]?.distance).toBeCloseTo(36);
    for (let index = 0; index < 100; index++) scene.controller.wheel({ deltaY: -100, deltaMode: 0 });
    expect(scene.cameraStore.current()?.camera.distance).toBe(2);
    for (let index = 0; index < 100; index++) scene.controller.wheel({ deltaY: 100, deltaMode: 0 });
    expect(scene.cameraStore.current()?.camera.distance).toBe(120);
  });
});

describe("клавиши", () => {
  it("W, E и R переключают вид ручек, когда ручки доступны", () => {
    const scene = setup();
    expect(scene.controller.keyDown(key("KeyW"))).toBe(true);
    expect(scene.controller.keyDown(key("KeyE"))).toBe(true);
    expect(scene.controller.keyDown(key("KeyR"))).toBe(true);
    expect(scene.modes).toEqual(["translate", "rotate", "scale"]);
  });

  it("с Ctrl, в повторе и в идущей партии клавиши до игры и браузера доходят как были", () => {
    const scene = setup();
    expect(scene.controller.keyDown(key("KeyR", { ctrlKey: true }))).toBe(false);
    expect(setup({ areHandlesAvailable: false }).controller.keyDown(key("KeyW"))).toBe(false);
    expect(setup({ isInputLocked: true }).controller.keyDown(key("KeyE"))).toBe(false);
    expect(scene.modes).toEqual([]);
  });

  it("F подводит камеру к выбранному объекту", () => {
    const scene = setup();
    scene.state.fitted = { target: [4, 5, 1], yaw: 0, pitch: 55, distance: 9 };
    expect(scene.controller.keyDown(key("KeyF"))).toBe(true);
    expect(scene.fitCalls).toEqual([7]);
    expect(scene.cameras).toEqual([{ target: [4, 5], yaw: 0, pitch: 55, distance: 9 }]);
  });

  it("F без выбора ничего не делает; на паузе камера редактора не видна и F до неё не доходит", () => {
    const none = setup({ selectedIndex: null });
    none.controller.keyDown(key("KeyF"));
    expect(none.fitCalls).toEqual([]);
    const paused = setup({ isEditorCameraActive: false });
    expect(paused.controller.keyDown(key("KeyF"))).toBe(false);
    expect(paused.fitCalls).toEqual([]);
  });

  it("Esc без жеста никому не мешает", () => {
    expect(setup().controller.keyDown(key("Escape"))).toBe(false);
  });
});

describe("жест бросается извне", () => {
  it("после abandonGesture отпускание не пишет действие", () => {
    const scene = setup();
    const commit = vi.fn();
    scene.context.onCommitPlacement = commit;
    scene.controller.pointerDown(pointer(150, 100));
    scene.controller.pointerMove(pointer(170, 120));
    scene.controller.abandonGesture();
    scene.controller.pointerUp(release(170, 120));
    expect(commit).not.toHaveBeenCalled();
  });
});

const RAISE: BrushSettings = { kind: "raise", size: 4, strength: 50 };
/** Точка сетки под (100, 100): клетка сцены (5, 5) — столбец 10, строка 10 сетки 25 × 25. */
const CENTER = 10 * 25 + 10;

/** Кадры страницы по 100 мс с `fromMs` по `toMs` включительно. */
function runFrames(scene: ReturnType<typeof setup>, fromMs: number, toMs: number): void {
  for (let time = fromMs; time <= toMs; time += 100) scene.controller.strokeFrame(time);
}

/** Отпечатки без выбора, как их отдаёт страница, когда ничего не выбрано. */
function setupImprints(): ImprintContext {
  return { isEditable: true, entries: [], selectedIndex: null, placing: null, onSelect: () => {}, onPlace: () => {}, onCommit: () => {}, onActiveChange: () => {} };
}

function isFlat(heights: Float64Array | undefined): boolean {
  return heights !== undefined && Array.from(heights).every((height) => height === 0);
}

describe("мазок кисти рельефа", () => {
  it("нажатие лепит, а не выбирает: объект не выбирается, выбор не снимается, ручка под указателем не берётся", () => {
    const scene = setup({ brush: RAISE });
    scene.state.pickedId = 9;
    expect(scene.controller.pointerDown(pointer(150, 100))).toBe(true);
    expect(scene.selections).toEqual([]);
    expect(scene.strokeStates).toEqual([true]);
  });

  it("луч мимо рельефа: мазка нет, а повели на землю с нажатой кнопкой — рисовать не начинает", () => {
    const scene = setup({ brush: RAISE });
    scene.state.isSkyUnderPointer = true;
    expect(scene.controller.pointerDown(pointer(100, 100))).toBe(false);
    scene.state.isSkyUnderPointer = false;
    scene.controller.pointerMove(pointer(100, 100));
    runFrames(scene, 100, 500);
    expect(scene.terrainSets).toEqual([]);
    expect(scene.strokeStates).toEqual([]);
  });

  it("держишь на месте — холм растёт: за секунду кадрами по 0,1 середина поднимается на клетку, движок получает высоты в каждом кадре", () => {
    const scene = setup({ brush: RAISE });
    scene.controller.pointerDown(pointer(100, 100));
    runFrames(scene, 100, 1000);
    expect(scene.terrainSets).toHaveLength(10);
    expect(scene.terrainSets[9]?.heights[CENTER]).toBeCloseTo(1, 9);
    expect(scene.terrainSets[9]?.heights[0]).toBe(0);
  });

  it("кадр дольше 0,1 секунды считается за 0,1", () => {
    const scene = setup({ brush: RAISE });
    scene.controller.pointerDown(pointer(100, 100));
    scene.controller.strokeFrame(5000);
    expect(scene.terrainSets[0]?.heights[CENTER]).toBeCloseTo(0.1, 9);
  });

  it("Shift смотрится в каждом кадре: нажал посреди мазка — дальше опускает, отпустил — снова поднимает", () => {
    const scene = setup({ brush: RAISE });
    scene.controller.pointerDown(pointer(100, 100));
    runFrames(scene, 100, 200);
    scene.controller.keyDown(key("ShiftLeft", { shiftKey: true }));
    runFrames(scene, 300, 400);
    expect(scene.terrainSets[3]?.heights[CENTER]).toBeCloseTo(0, 9);
    scene.controller.keyUp(key("ShiftLeft"));
    runFrames(scene, 500, 500);
    expect(scene.terrainSets[4]?.heights[CENTER]).toBeCloseTo(0.1, 9);
    scene.controller.pointerMove(pointer(100, 100, { shiftKey: true }));
    runFrames(scene, 600, 600);
    expect(scene.terrainSets[5]?.heights[CENTER]).toBeCloseTo(0, 9);
  });

  it("на отпечатке кисть лепит итоговую землю: в файл идёт прирост итоговой высоты, отпечатки мазок не трогает", () => {
    const stamps = [{ stamp: "beluha", position: [6, 6], size: [4, 4], height: 3 }];
    const scene = setup({ brush: RAISE, imprints: { ...setupImprints(), entries: stamps } });
    scene.state.effective = new Float64Array(25 * 25).fill(3);
    scene.controller.pointerDown(pointer(100, 100));
    runFrames(scene, 100, 1000);
    expect(scene.terrainSets[9]?.heights[CENTER]).toBeCloseTo(1, 9);
    expect(scene.terrainSets[9]?.heights[0]).toBe(0);
    expect(scene.terrainSets[9]?.stamps).toBe(stamps);
    scene.controller.pointerUp(release(100, 100));
    expect(scene.terrainCommits[0]?.[CENTER]).toBeCloseTo(1, 9);
  });

  it("«Выровнять» на отпечатке ведёт видимую землю к итоговой высоте начала мазка, а файл — на ту же разницу", () => {
    const scene = setup({ brush: { kind: "level", size: 4, strength: 50 } });
    scene.state.effective = new Float64Array(25 * 25).fill(3);
    scene.state.groundZ = 5;
    scene.controller.pointerDown(pointer(100, 100));
    runFrames(scene, 100, 500);
    expect(scene.terrainSets[4]?.heights[CENTER]).toBeCloseTo(2 * (1 - Math.exp(-2.5)), 9);
  });

  it("Esc посреди мазка на отпечатке возвращает высоты файла без прироста", () => {
    const scene = setup({ brush: RAISE });
    scene.state.effective = new Float64Array(25 * 25).fill(3);
    scene.controller.pointerDown(pointer(100, 100));
    runFrames(scene, 100, 300);
    scene.controller.keyDown(key("Escape"));
    expect(isFlat(scene.terrainSets.at(-1)?.heights)).toBe(true);
    expect(scene.terrainCommits).toEqual([]);
  });

  it("«Выровнять» ведёт к высоте рельефа в точке нажатия", () => {
    const scene = setup({ brush: { kind: "level", size: 4, strength: 50 } });
    scene.state.groundZ = 2;
    scene.controller.pointerDown(pointer(100, 100));
    runFrames(scene, 100, 500);
    expect(scene.terrainSets[4]?.heights[CENTER]).toBeCloseTo(2 * (1 - Math.exp(-2.5)), 9);
  });

  it("быстрый увод указателя между кадрами — след сплошной, без пропусков", () => {
    const scene = setup({ brush: RAISE });
    scene.controller.pointerDown(pointer(100, 100));
    scene.controller.pointerMove(pointer(170, 100));
    scene.controller.strokeFrame(100);
    const row = scene.terrainSets[0]?.heights.slice(10 * 25 + 10, 10 * 25 + 25) as Float64Array;
    expect(Array.from(row).every((height) => height > 0)).toBe(true);
  });

  it("отпускание — одно действие: высоты всей сетки уходят странице", () => {
    const scene = setup({ brush: RAISE });
    scene.controller.pointerDown(pointer(100, 100));
    runFrames(scene, 100, 1000);
    scene.controller.pointerUp(release(100, 100));
    expect(scene.terrainCommits).toHaveLength(1);
    expect(scene.terrainCommits[0]?.[CENTER]).toBeCloseTo(1, 9);
    expect(scene.strokeStates).toEqual([true, false]);
  });

  it("Esc без мазка снимает кисть и возвращает прежний вид ручек", () => {
    const scene = setup({ brush: RAISE, handleMode: "rotate" });
    expect(scene.controller.keyDown(key("Escape"))).toBe(true);
    expect(scene.modes).toEqual(["rotate"]);
  });

  it("Esc без кисти и без жеста ничего не делает", () => {
    const scene = setup();
    expect(scene.controller.keyDown(key("Escape"))).toBe(false);
    expect(scene.modes).toEqual([]);
  });

  it("нажали и отпустили, не держа, — не действие", () => {
    const scene = setup({ brush: RAISE });
    scene.controller.pointerDown(pointer(100, 100));
    scene.controller.pointerUp(release(100, 100));
    expect(scene.terrainCommits).toEqual([]);
    expect(scene.terrainSets).toEqual([]);
    expect(scene.strokeStates).toEqual([true, false]);
  });

  it("изменение меньше сотой — не действие, рельеф возвращается как был", () => {
    const scene = setup({ brush: { kind: "raise", size: 4, strength: 1 } });
    scene.controller.pointerDown(pointer(100, 100));
    scene.controller.strokeFrame(1);
    scene.controller.pointerUp(release(100, 100));
    expect(scene.terrainCommits).toEqual([]);
    expect(isFlat(scene.terrainSets.at(-1)?.heights)).toBe(true);
  });

  it("Esc посреди мазка возвращает рельеф до мазка: действия нет, отпускание потом ничего не пишет", () => {
    const scene = setup({ brush: RAISE });
    scene.controller.pointerDown(pointer(100, 100));
    runFrames(scene, 100, 500);
    expect(scene.controller.keyDown(key("Escape"))).toBe(true);
    expect(isFlat(scene.terrainSets.at(-1)?.heights)).toBe(true);
    expect(scene.modes).toEqual([]);
    scene.controller.pointerUp(release(100, 100));
    expect(scene.terrainCommits).toEqual([]);
    expect(scene.strokeStates).toEqual([true, false]);
  });

  it("сброс указателя браузером возвращает рельеф до мазка", () => {
    const scene = setup({ brush: RAISE });
    scene.controller.pointerDown(pointer(100, 100));
    runFrames(scene, 100, 300);
    scene.controller.pointerCancel(pointer(100, 100));
    expect(isFlat(scene.terrainSets.at(-1)?.heights)).toBe(true);
    expect(scene.terrainCommits).toEqual([]);
  });

  it("указатель ушёл с холста с нажатой кнопкой — мазок не прерывается", () => {
    const scene = setup({ brush: RAISE });
    scene.controller.pointerDown(pointer(100, 100));
    scene.controller.pointerLeave();
    runFrames(scene, 100, 200);
    expect(scene.terrainSets).toHaveLength(2);
    expect(scene.strokeStates).toEqual([true]);
  });

  it("указатель ушёл на небо — кадры ничего не меняют, вернулся — мазок продолжается с новой точки без пути через пустоту", () => {
    const scene = setup({ brush: RAISE });
    scene.controller.pointerDown(pointer(100, 100));
    scene.state.isSkyUnderPointer = true;
    runFrames(scene, 100, 200);
    expect(scene.terrainSets).toEqual([]);
    scene.state.isSkyUnderPointer = false;
    scene.controller.pointerMove(pointer(60, 60));
    scene.controller.strokeFrame(300);
    const heights = scene.terrainSets[0]?.heights as Float64Array;
    expect(heights[CENTER]).toBe(0);
    expect(heights[1 * 25 + 1]).toBeGreaterThan(0);
  });

  it("W, E и R без мазка зовут смену вида ручек — страница снимет кисть; посреди мазка не переключают", () => {
    const scene = setup({ brush: RAISE });
    expect(scene.controller.keyDown(key("KeyW"))).toBe(true);
    expect(scene.modes).toEqual(["translate"]);
    scene.controller.pointerDown(pointer(100, 100));
    expect(scene.controller.keyDown(key("KeyE"))).toBe(false);
    expect(scene.modes).toEqual(["translate"]);
  });

  it("движок отказал в set_terrain — мазок бросается, страница узнаёт, действия нет", () => {
    const scene = setup({ brush: RAISE });
    scene.state.setTerrainError = "идёт партия";
    scene.controller.pointerDown(pointer(100, 100));
    runFrames(scene, 100, 300);
    scene.controller.pointerUp(release(100, 100));
    expect(scene.terrainSets).toHaveLength(1);
    expect(scene.terrainCommits).toEqual([]);
    expect(scene.strokeStates).toEqual([true, false]);
  });

  it("мазок бросается извне (партия пошла, файлы пересобраны) — рельеф не возвращается, действия нет", () => {
    const scene = setup({ brush: RAISE });
    scene.controller.pointerDown(pointer(100, 100));
    runFrames(scene, 100, 200);
    scene.controller.abandonGesture();
    scene.controller.pointerUp(release(100, 100));
    expect(scene.terrainSets).toHaveLength(2);
    expect(scene.terrainCommits).toEqual([]);
    expect(scene.strokeStates).toEqual([true, false]);
  });

  it("сцены под мазком не стало — страница всё равно узнаёт, что мазок кончился", () => {
    const scene = setup({ brush: RAISE });
    let current: SpaceSceneContext | null = scene.context;
    const controller = createSpaceSceneController(() => current as SpaceSceneContext);
    controller.pointerDown(pointer(100, 100));
    current = null;
    controller.abandonGesture();
    expect(scene.strokeStates).toEqual([true, false]);
  });

  it("средняя кнопка и колесо ведут камеру и при кисти", () => {
    const scene = setup({ brush: RAISE });
    expect(scene.controller.pointerDown(pointer(100, 100, { button: 1, buttons: 4 }))).toBe(true);
    scene.controller.pointerMove(pointer(150, 100, { button: -1, buttons: 4 }));
    expect(scene.cameras).toHaveLength(1);
    expect(scene.controller.wheel({ deltaY: -100, deltaMode: 0 })).toBe(true);
    expect(scene.strokeStates).toEqual([]);
  });
});

/** Холст, что записывает вызовы, — рисование трёхмерной сцены проверяется по тому, что она спросила у движка. */
function recordingCanvasContext(): CanvasRenderingContext2D {
  const target: Record<string, unknown> = { canvas: { width: 800, height: 600 } };
  return new Proxy(target, {
    get: (record, name: string) => (name in record ? record[name] : () => ({ width: 10 })),
    set: (record, name: string, value: unknown) => {
      record[name] = value;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
}

describe("круг кисти, рамка и ручки", () => {
  it("круг — 64 точки на рельефе и метка в середине, идёт за указателем", () => {
    const scene = setup({ brush: RAISE });
    const screenPoint = vi.spyOn(scene.engine, "screen_point");
    const terrainHeight = vi.spyOn(scene.engine, "terrain_height");
    scene.controller.draw(recordingCanvasContext(), 1);
    expect(screenPoint).not.toHaveBeenCalled();
    scene.controller.pointerMove(pointer(100, 100, { buttons: 0 }));
    scene.controller.draw(recordingCanvasContext(), 1);
    expect(terrainHeight).toHaveBeenCalledTimes(64);
    expect(screenPoint).toHaveBeenCalledTimes(64 + 1);
  });

  it("указатель ушёл с холста — круга нет; луч мимо рельефа — тоже", () => {
    const scene = setup({ brush: RAISE });
    const screenPoint = vi.spyOn(scene.engine, "screen_point");
    scene.controller.pointerMove(pointer(100, 100, { buttons: 0 }));
    scene.controller.pointerLeave();
    scene.controller.draw(recordingCanvasContext(), 1);
    scene.controller.pointerMove(pointer(100, 100, { buttons: 0 }));
    scene.state.isSkyUnderPointer = true;
    scene.controller.draw(recordingCanvasContext(), 1);
    expect(screenPoint).not.toHaveBeenCalled();
  });

  it("при выбранной кисти рамка выбранного рисуется, ручки — нет", () => {
    const scene = setup({ brush: RAISE });
    const objectRect = vi.spyOn(scene.engine, "object_rect");
    const screenPoint = vi.spyOn(scene.engine, "screen_point");
    scene.controller.draw(recordingCanvasContext(), 1);
    expect(objectRect).toHaveBeenCalled();
    expect(screenPoint).not.toHaveBeenCalled();
  });

  it("без кисти ручки рисуются", () => {
    const scene = setup();
    const screenPoint = vi.spyOn(scene.engine, "screen_point");
    scene.controller.draw(recordingCanvasContext(), 1);
    expect(screenPoint).toHaveBeenCalled();
  });
});

describe("мазок «Покрасить»", () => {
  function paintSetup() {
    const commits: PaintResult[] = [];
    const restores: string[] = [];
    const paint: PaintContext = {
      material: "rock",
      size: 4,
      strength: 50,
      covers: [{ material: "grass" }],
      masks: {},
      sceneSize: { width: 12, height: 12 },
      hasTerrainFile: true,
      tintPath: null,
      onCommit: (result) => commits.push(result),
      onRestore: () => restores.push("restore"),
    };
    return { scene: setup({ paint }), commits, restores };
  }

  it("нажатие красит, а не выбирает: объект не выбирается, выбор не снимается", () => {
    const { scene } = paintSetup();
    scene.state.pickedId = 9;

    expect(scene.controller.pointerDown(pointer(150, 100))).toBe(true);

    expect(scene.selections).toEqual([]);
    expect(scene.strokeStates).toEqual([true]);
    expect(scene.terrainSets).toEqual([]);
  });

  it("кадры страницы красят, пока кнопка нажата: новый слой уходит движку, а отпускание отдаёт мазок странице одним действием", () => {
    const { scene, commits } = paintSetup();
    scene.controller.pointerDown(pointer(100, 100));

    runFrames(scene, 100, 300);
    scene.controller.pointerUp(release(100, 100));

    expect(scene.coverSets).toHaveLength(3);
    expect(scene.coverSets[2]?.covers).toEqual([{ material: "grass" }, { material: "rock", mask: "terrain/rock.png" }]);
    expect(commits).toHaveLength(1);
    expect(Object.keys(commits[0]?.masks ?? {})).toEqual(["terrain/rock.png"]);
    expect(scene.strokeStates).toEqual([true, false]);
  });

  it("Esc во время мазка откатывает его; Esc без мазка снимает «Покрасить» — вместо неё вид ручек", () => {
    const { scene, commits } = paintSetup();
    scene.controller.pointerDown(pointer(100, 100));
    runFrames(scene, 100, 100);

    expect(scene.controller.keyDown(key("Escape"))).toBe(true);

    expect(scene.coverSets.at(-1)?.covers).toEqual([{ material: "grass" }]);
    expect(commits).toEqual([]);
    expect(scene.modes).toEqual([]);
    expect(scene.controller.keyDown(key("Escape"))).toBe(true);
    expect(scene.modes).toEqual(["translate"]);
  });

  it("во время мазка W, E, R вид ручек не меняют", () => {
    const { scene } = paintSetup();
    scene.controller.pointerDown(pointer(100, 100));

    expect(scene.controller.keyDown(key("KeyE"))).toBe(false);

    expect(scene.modes).toEqual([]);
  });

  it("ручки не рисуются и не берутся, круг кисти рисуется", () => {
    const { scene } = paintSetup();
    const screenPoint = vi.spyOn(scene.engine, "screen_point");
    const terrainHeight = vi.spyOn(scene.engine, "terrain_height");
    scene.controller.pointerMove(pointer(100, 100, { buttons: 0 }));

    scene.controller.draw(recordingCanvasContext(), 1);

    expect(terrainHeight).toHaveBeenCalled();
    expect(screenPoint.mock.calls.every(([, , z]) => z === 0)).toBe(true);
  });

  it("мир собран заново (правка файла снаружи) — мазок бросается и сообщает об этом странице", () => {
    const { scene, commits } = paintSetup();
    scene.controller.pointerDown(pointer(100, 100));

    scene.controller.abandonGesture();

    scene.controller.pointerUp(release(100, 100));

    expect(scene.strokeStates).toEqual([true, false]);
    expect(commits).toEqual([]);
  });
});
