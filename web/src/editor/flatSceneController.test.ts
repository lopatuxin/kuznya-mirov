import { describe, expect, it } from "vitest";
import { createFlatCameraStore, type FlatCameraState } from "./editorCamera";
import {
  createFlatSceneController,
  type FlatSceneContext,
  type FlatSceneController,
  type FlatSceneEngine,
} from "./flatSceneController";
import type { PlacementChange } from "./objectPlacement";
import type { KeyInput, PointerInput } from "./spaceSceneController";

const CANVAS_WIDTH = 1200;
const CANVAS_HEIGHT = 600;
const SCENE = { width: 160, height: 24 };
const WHOLE_SCENE: FlatCameraState = { center: [80, 12], view_height: 80 };
const NO_KEYS: KeyInput = { code: "", ctrlKey: false, altKey: false, metaKey: false, shiftKey: false };

/** Масштаб кадра при открытии: 600 / 80 = 7,5 точки на клетку, середина сцены в середине холста. */
const SCALE = 7.5;

function pointer(x: number, y: number, extra: Partial<PointerInput> = {}): PointerInput {
  return { pointerId: 1, button: 0, buttons: 1, x, y, ctrlKey: false, shiftKey: false, timeStamp: 0, ...extra };
}

function release(x: number, y: number, extra: Partial<PointerInput> = {}): PointerInput {
  return pointer(x, y, { buttons: 0, ...extra });
}

function key(code: string, extra: Partial<KeyInput> = {}): KeyInput {
  return { ...NO_KEYS, code, ...extra };
}

type SceneObject = Record<string, unknown>;

/** Плоский кадр без перспективы: точка холста ↔ клетка по камере, слой сдвинут от середины сцены на `(center − mid) × (1 − parallax)`. */
function setup(overrides: Partial<FlatSceneContext> = {}, objects: Record<number, SceneObject> = {}) {
  const worldObjects: Record<number, SceneObject> = { 7: { position: [10, 15], size: [4, 2] }, ...objects };
  const moves: [number, number, number][] = [];
  const transforms: [number, Record<string, unknown>][] = [];
  const state = { camera: { ...WHOLE_SCENE } as FlatCameraState, pickedId: undefined as number | undefined, fitted: undefined as FlatCameraState | undefined };

  function frame(parallax: number): { scale: number; offset: [number, number]; shift: [number, number] } {
    const scale = CANVAS_HEIGHT / state.camera.view_height;
    const offset: [number, number] = [CANVAS_WIDTH / 2 - state.camera.center[0] * scale, CANVAS_HEIGHT / 2 - state.camera.center[1] * scale];
    const p = Math.max(0, parallax);
    const shift: [number, number] = [(state.camera.center[0] - SCENE.width / 2) * (1 - p), (state.camera.center[1] - SCENE.height / 2) * (1 - p)];
    return { scale, offset, shift };
  }

  const engine = {
    object_at: () => state.pickedId,
    object_rect: (id: number) => {
      const properties = worldObjects[id];
      if (properties === undefined) return undefined;
      const { scale, offset, shift } = frame(typeof properties.parallax === "number" ? properties.parallax : 1);
      const position = properties.position as [number, number];
      const size = properties.size as [number, number];
      return { x: offset[0] + (position[0] + shift[0]) * scale, y: offset[1] + (position[1] + shift[1]) * scale, width: size[0] * scale, height: size[1] * scale };
    },
    scene_point: (x: number, y: number, parallax: number) => {
      const { scale, offset, shift } = frame(parallax);
      return [(x - offset[0]) / scale - shift[0], (y - offset[1]) / scale - shift[1]];
    },
    screen_point: (x: number, y: number) => {
      const { scale, offset } = frame(1);
      return [offset[0] + x * scale, offset[1] + y * scale];
    },
    move_object: (id: number, x: number, y: number) => moves.push([id, x, y]),
    transform_object: (id: number, transform: Record<string, unknown>) => transforms.push([id, transform]),
    editor_camera: (camera: FlatCameraState) => {
      state.camera = camera;
    },
    fit_camera: (id?: number | null) => (id === null || id === undefined ? WHOLE_SCENE : state.fitted),
  } as unknown as FlatSceneEngine;

  const cameraStore = createFlatCameraStore();
  cameraStore.restore(engine);

  const commits: [number, PlacementChange[]][] = [];
  const selections: (number | null)[] = [];
  const modes: string[] = [];
  const context: FlatSceneContext = {
    engine,
    cameraStore,
    sceneSize: SCENE,
    isInputLocked: false,
    isEditorCameraActive: true,
    areHandlesAvailable: true,
    selectedIndex: 7,
    selectedLabel: null,
    handleMode: "translate",
    getObjectProperties: (id) => worldObjects[id] ?? null,
    onSelect: (index) => selections.push(index),
    onHandleModeChange: (mode) => modes.push(mode),
    onCommitPlacement: (index, changes) => commits.push([index, changes]),
    ...overrides,
  };
  const controller: FlatSceneController = createFlatSceneController(() => context);
  return { controller, context, state, moves, transforms, commits, selections, modes, cameraStore, engine, worldObjects };
}

/** Объект 7 при камере на всю сцену: `position [10, 15]`, `size [4, 2]` — прямоугольник от (75, 322,5) шириной 30 и высотой 15, середина (90, 330). */
const CENTER: [number, number] = [90, 330];

describe("колесо", () => {
  it("щелчок на себя — высота × 0,9, точка под указателем остаётся под ним (пример требования 6)", () => {
    const { controller, state, engine } = setup();

    expect(controller.wheel({ deltaY: -100, deltaMode: 0, ctrlKey: false, x: 300, y: 300 })).toBe(true);

    expect(state.camera.view_height).toBeCloseTo(72);
    expect(state.camera.center[0]).toBeCloseTo(76);
    expect(state.camera.center[1]).toBeCloseTo(12);
    const under = engine.scene_point(300, 300, 1) as [number, number];
    expect(under[0]).toBeCloseTo(40);
    expect(under[1]).toBeCloseTo(12);
  });

  it("щелчок от себя — высота ÷ 0,9", () => {
    const { controller, state } = setup();

    controller.wheel({ deltaY: 100, deltaMode: 0, ctrlKey: false, x: 600, y: 300 });

    expect(state.camera.view_height).toBeCloseTo(80 / 0.9);
  });

  it("Ctrl+колесо приближает так же и гасит масштаб страницы", () => {
    const { controller, state } = setup();

    expect(controller.wheel({ deltaY: -100, deltaMode: 0, ctrlKey: true, x: 300, y: 300 })).toBe(true);

    expect(state.camera.view_height).toBeCloseTo(72);
  });

  it("высота вида — от 2 клеток до трёх высот всей сцены, то есть до 240", () => {
    const { controller, state } = setup();

    controller.wheel({ deltaY: -100000, deltaMode: 0, ctrlKey: false, x: 600, y: 300 });
    expect(state.camera.view_height).toBe(2);
    controller.wheel({ deltaY: 100000, deltaMode: 0, ctrlKey: false, x: 600, y: 300 });
    expect(state.camera.view_height).toBeCloseTo(240);
  });

  it("в партии и на паузе колесо не действует", () => {
    expect(setup({ isInputLocked: true }).controller.wheel({ deltaY: -100, deltaMode: 0, ctrlKey: false, x: 0, y: 0 })).toBe(false);
    const paused = setup({ isEditorCameraActive: false });
    expect(paused.controller.wheel({ deltaY: -100, deltaMode: 0, ctrlKey: false, x: 0, y: 0 })).toBe(false);
    expect(paused.state.camera).toEqual(WHOLE_SCENE);
  });
});

describe("средняя кнопка", () => {
  it("сцена идёт за мышью: взятая точка остаётся под указателем", () => {
    const { controller, state, engine } = setup();
    const grabbed = engine.scene_point(300, 300, 1) as [number, number];

    expect(controller.pointerDown(pointer(300, 300, { button: 1, buttons: 4 }))).toBe(true);
    controller.pointerMove(pointer(420, 330, { button: 1, buttons: 4 }));

    const under = engine.scene_point(420, 330, 1) as [number, number];
    expect(under[0]).toBeCloseTo(grabbed[0]);
    expect(under[1]).toBeCloseTo(grabbed[1]);
    expect(state.camera.view_height).toBe(80);
    expect(state.camera.center[0]).toBeCloseTo(80 - 120 / SCALE);
  });

  it("во время жеста левой кнопки второго жеста не начинает", () => {
    const { controller } = setup();
    expect(controller.pointerDown(pointer(...CENTER))).toBe(true);

    expect(controller.pointerDown(pointer(300, 300, { pointerId: 2, button: 1, buttons: 5 }))).toBe(false);
  });

  it("в партии, на паузе и в повторе камеру не водит", () => {
    expect(setup({ isEditorCameraActive: false }).controller.pointerDown(pointer(300, 300, { button: 1, buttons: 4 }))).toBe(false);
    expect(setup({ isInputLocked: true }).controller.pointerDown(pointer(300, 300, { button: 1, buttons: 4 }))).toBe(false);
  });
});

describe("клавиша F", () => {
  it("подводит камеру к выбранному объекту", () => {
    const { controller, state } = setup();
    state.fitted = { center: [168, 26], view_height: 14.4 };

    expect(controller.keyDown(key("KeyF"))).toBe(true);

    expect(state.camera).toEqual({ center: [168, 26], view_height: 14.4 });
  });

  it("без выбранного объекта и когда движок объекта не знает, камера прежняя", () => {
    const withoutSelection = setup({ selectedIndex: null });
    withoutSelection.state.fitted = { center: [1, 1], view_height: 2 };
    withoutSelection.controller.keyDown(key("KeyF"));
    expect(withoutSelection.state.camera).toEqual(WHOLE_SCENE);

    const unknown = setup();
    unknown.controller.keyDown(key("KeyF"));
    expect(unknown.state.camera).toEqual(WHOLE_SCENE);
  });

  it("на паузе, в повторе и в партии ничего не делает", () => {
    const paused = setup({ isEditorCameraActive: false });
    paused.state.fitted = { center: [1, 1], view_height: 2 };

    expect(paused.controller.keyDown(key("KeyF"))).toBe(false);
    expect(paused.state.camera).toEqual(WHOLE_SCENE);
  });
});

describe("клавиши видов ручек", () => {
  it("W и R включают перенос и масштаб, E ничего не делает", () => {
    const { controller, modes } = setup();

    expect(controller.keyDown(key("KeyW"))).toBe(true);
    expect(controller.keyDown(key("KeyR"))).toBe(true);
    expect(controller.keyDown(key("KeyE"))).toBe(false);

    expect(modes).toEqual(["translate", "scale"]);
  });

  it("без доступных ручек и в партии клавиши не действуют", () => {
    expect(setup({ areHandlesAvailable: false }).controller.keyDown(key("KeyR"))).toBe(false);
    expect(setup({ isInputLocked: true }).controller.keyDown(key("KeyR"))).toBe(false);
  });
});

describe("перенос ручками", () => {
  it("стрелка x двигает только по x: указатель на (2,346; 1,2) клетки — [12,35; 15] (пример требования 17)", () => {
    const { controller, commits, moves } = setup();
    const start: [number, number] = [CENTER[0] + 50, CENTER[1]];

    expect(controller.pointerDown(pointer(...start))).toBe(true);
    controller.pointerMove(pointer(start[0] + 2.346 * SCALE, start[1] + 1.2 * SCALE));
    controller.pointerUp(release(start[0] + 2.346 * SCALE, start[1] + 1.2 * SCALE));

    expect(moves.at(-1)).toEqual([7, 12.35, 15]);
    expect(commits).toEqual([[7, [{ key: "position", value: [12.35, 15], previous: [10, 15] }]]]);
  });

  it("с Ctrl — до целой клетки: [12, 15]", () => {
    const { controller, commits } = setup();
    const start: [number, number] = [CENTER[0] + 50, CENTER[1]];

    controller.pointerDown(pointer(...start));
    controller.pointerMove(pointer(start[0] + 2.346 * SCALE, start[1] + 1.2 * SCALE, { ctrlKey: true }));
    controller.pointerUp(release(start[0] + 2.346 * SCALE, start[1], { ctrlKey: true }));

    expect(commits[0]?.[1][0]?.value).toEqual([12, 15]);
  });

  it("стрелка y двигает только по y", () => {
    const { controller, commits } = setup();
    const start: [number, number] = [CENTER[0], CENTER[1] + 50];

    controller.pointerDown(pointer(...start));
    controller.pointerMove(pointer(start[0] + 30, start[1] + 15));
    controller.pointerUp(release(start[0] + 30, start[1] + 15));

    expect(commits[0]?.[1][0]?.value).toEqual([10, 17]);
  });

  it("квадрат в середине двигает свободно", () => {
    const { controller, commits } = setup();

    controller.pointerDown(pointer(...CENTER));
    controller.pointerMove(pointer(CENTER[0] + 2.346 * SCALE, CENTER[1] + 1.2 * SCALE));
    controller.pointerUp(release(CENTER[0] + 2.346 * SCALE, CENTER[1] + 1.2 * SCALE));

    expect(commits[0]?.[1][0]?.value).toEqual([12.35, 16.2]);
  });

  it("объект слоя идёт на столько же клеток, на сколько указатель", () => {
    const layer = { 9: { position: [90, 11.5], size: [24, 8], parallax: 0.25 } };
    const { controller, commits, state, engine } = setup({ selectedIndex: 9 }, layer);
    state.camera = { center: [168, 26], view_height: 14.4 };
    const rect = engine.object_rect(9) as { x: number; y: number; width: number; height: number };
    const center: [number, number] = [rect.x + rect.width / 2, rect.y + rect.height / 2];
    const scale = CANVAS_HEIGHT / 14.4;

    controller.pointerDown(pointer(...center));
    controller.pointerMove(pointer(center[0] + 4 * scale, center[1]));
    controller.pointerUp(release(center[0] + 4 * scale, center[1]));

    const [x, y] = commits[0]?.[1][0]?.value as number[];
    expect(x).toBeCloseTo(94);
    expect(y).toBe(11.5);
  });
});

describe("масштаб ручками", () => {
  /** Камера ближе: 50 точек на клетку, объект 7 — прямоугольник (500, 250) шириной 200 и высотой 100. */
  const ZOOMED: FlatCameraState = { center: [12, 16], view_height: 12 };
  const ZOOMED_SCALE = 50;
  const HANDLES = {
    left: [500, 300],
    right: [700, 300],
    top: [600, 250],
    "top-left": [500, 250],
    "bottom-right": [700, 350],
  } as const;

  function scaleSetup() {
    const result = setup({ handleMode: "scale" });
    result.cameraStore.update(result.engine, ZOOMED);
    return result;
  }

  /** Тянет ручку на `delta` клеток и отдаёт то, что записано одним действием. */
  function drag(handle: keyof typeof HANDLES, delta: [number, number], extra: Partial<PointerInput> = {}) {
    const result = scaleSetup();
    const from = HANDLES[handle];
    const to: [number, number] = [from[0] + delta[0] * ZOOMED_SCALE, from[1] + delta[1] * ZOOMED_SCALE];
    expect(result.controller.pointerDown(pointer(from[0], from[1]))).toBe(true);
    result.controller.pointerMove(pointer(...to, extra));
    result.controller.pointerUp(release(...to, extra));
    const changes = result.commits[0]?.[1] ?? [];
    return { position: changes.find((change) => change.key === "position")?.value, size: changes.find((change) => change.key === "size")?.value, result };
  }

  it("правая сторона на +2 по x — size [6, 2], position не меняется", () => {
    const { position, size } = drag("right", [2, 0]);

    expect(size).toEqual([6, 2]);
    expect(position).toBeUndefined();
  });

  it("левая сторона на +1 — size [3, 2], position [11, 15]", () => {
    const { position, size } = drag("left", [1, 0]);

    expect(size).toEqual([3, 2]);
    expect(position).toEqual([11, 15]);
  });

  it("верхняя сторона на −1 — size [4, 3], position [10, 14]", () => {
    const { position, size } = drag("top", [0, -1]);

    expect(size).toEqual([4, 3]);
    expect(position).toEqual([10, 14]);
  });

  it("правый нижний угол на (+2, +1) — size [6, 3], position не меняется", () => {
    const { position, size } = drag("bottom-right", [2, 1]);

    expect(size).toEqual([6, 3]);
    expect(position).toBeUndefined();
  });

  it("левый верхний угол на (−2, −1) — size [6, 3], position [8, 14]", () => {
    const { position, size } = drag("top-left", [-2, -1]);

    expect(size).toEqual([6, 3]);
    expect(position).toEqual([8, 14]);
  });

  it("правая сторона на −5 — size [0,1; 2]", () => {
    expect(drag("right", [-5, 0]).size).toEqual([0.1, 2]);
  });

  it("с Ctrl доля 1,47 округляется до 1,5", () => {
    expect(drag("right", [1.88, 0], { ctrlKey: true }).size).toEqual([6, 2]);
  });

  it("во время жеста мир сразу показывает новый размер, Esc возвращает прежний без действия", () => {
    const { controller, transforms, commits } = scaleSetup();

    controller.pointerDown(pointer(...HANDLES.right));
    controller.pointerMove(pointer(HANDLES.right[0] + 2 * ZOOMED_SCALE, HANDLES.right[1]));
    expect(transforms.at(-1)).toEqual([7, { position: [10, 15], size: [6, 2] }]);

    expect(controller.keyDown(key("Escape"))).toBe(true);
    expect(transforms.at(-1)).toEqual([7, { position: [10, 15], size: [4, 2] }]);
    controller.pointerUp(release(HANDLES.right[0] + 2 * ZOOMED_SCALE, HANDLES.right[1]));
    expect(commits).toEqual([]);
  });
});

describe("выбор и перенос самого объекта", () => {
  it("ручка под указателем важнее объекта под ним", () => {
    const { controller, selections, state } = setup();
    state.pickedId = 9;

    expect(controller.pointerDown(pointer(...CENTER))).toBe(true);

    expect(selections).toEqual([]);
  });

  it("нажатие на объект мимо ручек выбирает его и переносит при любом виде ручек", () => {
    const { controller, selections, state, commits } = setup({ selectedIndex: null, handleMode: "scale" }, { 9: { position: [20, 15], size: [4, 2] } });
    state.pickedId = 9;

    expect(controller.pointerDown(pointer(200, 330))).toBe(true);
    controller.pointerMove(pointer(200 + 15, 330));
    controller.pointerUp(release(200 + 15, 330));

    expect(selections).toEqual([9]);
    expect(commits).toEqual([[9, [{ key: "position", value: [22, 15], previous: [20, 15] }]]]);
  });

  it("сдвиг меньше порога не переносит и не пишет", () => {
    const { controller, commits, state } = setup({ selectedIndex: null });
    state.pickedId = 7;

    controller.pointerDown(pointer(...CENTER));
    controller.pointerMove(pointer(CENTER[0] + 2, CENTER[1]));
    controller.pointerUp(release(CENTER[0] + 2, CENTER[1]));

    expect(commits).toEqual([]);
  });

  it("щелчок по пустому месту снимает выбор", () => {
    const { controller, selections } = setup({ selectedIndex: null });

    expect(controller.pointerDown(pointer(900, 100))).toBe(false);

    expect(selections).toEqual([null]);
  });

  it("без доступных ручек выбор работает, а переноса нет", () => {
    const { controller, selections, state } = setup({ areHandlesAvailable: false, selectedIndex: null });
    state.pickedId = 7;

    expect(controller.pointerDown(pointer(...CENTER))).toBe(false);

    expect(selections).toEqual([7]);
  });

  it("Esc во время переноса возвращает объект на место без действия, а потеря жеста его бросает", () => {
    const { controller, moves, commits, state } = setup({ selectedIndex: null });
    state.pickedId = 7;
    controller.pointerDown(pointer(...CENTER));
    controller.pointerMove(pointer(CENTER[0] + 30, CENTER[1]));
    expect(moves.at(-1)).toEqual([7, 14, 15]);

    expect(controller.keyDown(key("Escape"))).toBe(true);
    expect(moves.at(-1)).toEqual([7, 10, 15]);
    controller.pointerUp(release(CENTER[0] + 30, CENTER[1]));
    expect(commits).toEqual([]);

    controller.pointerDown(pointer(...CENTER));
    controller.pointerMove(pointer(CENTER[0] + 30, CENTER[1]));
    controller.abandonGesture();
    controller.pointerUp(release(CENTER[0] + 30, CENTER[1]));
    expect(commits).toEqual([]);
  });

  it("колесо во время жеста приближает камеру, а жест продолжается: точка под указателем берётся по новой камере", () => {
    const { controller, commits } = setup();
    const start: [number, number] = [CENTER[0] + 50, CENTER[1]];
    controller.pointerDown(pointer(...start));

    controller.wheel({ deltaY: -100, deltaMode: 0, ctrlKey: false, x: 300, y: 300 });
    controller.pointerMove(pointer(...start));
    controller.pointerUp(release(...start));

    expect(commits[0]?.[1][0]?.value).toEqual([12.13, 15]);
  });
});

describe("рисование", () => {
  type Call = [string, ...unknown[]];

  function fakeContext(calls: Call[]): CanvasRenderingContext2D {
    const target: Record<string, unknown> = { canvas: { width: CANVAS_WIDTH, height: CANVAS_HEIGHT } };
    return new Proxy(target, {
      get: (object, property: string) => (property in object ? object[property] : (...args: unknown[]) => calls.push([property, ...args])),
    }) as unknown as CanvasRenderingContext2D;
  }

  it("пока сцену показывает камера редактора, рисуется линия по границе сцены от (0, 0) до (ширина, высота)", () => {
    const { controller } = setup({ selectedIndex: null });
    const calls: Call[] = [];

    controller.draw(fakeContext(calls), 1);

    const boundary = calls.filter(([name]) => name === "strokeRect");
    expect(boundary).toHaveLength(2);
    expect(boundary[0]).toEqual(["strokeRect", 0, 210, 1200, 180]);
  });

  it("в партии, на паузе и в повторе линии нет", () => {
    const { controller } = setup({ selectedIndex: null, isEditorCameraActive: false });
    const calls: Call[] = [];

    controller.draw(fakeContext(calls), 1);

    expect(calls.filter(([name]) => name === "strokeRect")).toHaveLength(0);
  });

  it("у выбранного объекта рисуются рамка и ручки; нет доступных ручек — только рамка", () => {
    const withHandles = setup();
    const withoutHandles = setup({ areHandlesAvailable: false });
    const handlesCalls: Call[] = [];
    const plainCalls: Call[] = [];

    withHandles.controller.draw(fakeContext(handlesCalls), 1);
    withoutHandles.controller.draw(fakeContext(plainCalls), 1);

    expect(handlesCalls.length).toBeGreaterThan(plainCalls.length);
    expect(plainCalls.some(([name]) => name === "fillRect")).toBe(true);
  });
});
