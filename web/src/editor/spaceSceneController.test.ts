import { describe, expect, it, vi } from "vitest";
import { createEditorCameraStore, type EditorCameraState } from "./editorCamera";
import type { PlacementChange } from "./objectPlacement";
import {
  createSpaceSceneController,
  type KeyInput,
  type PointerInput,
  type SpaceSceneContext,
  type SpaceSceneEngine,
} from "./spaceSceneController";

const START_CAMERA: EditorCameraState = { target: [16, 12], yaw: 0, pitch: 55, distance: 40 };
const NO_KEYS: KeyInput = { code: "", ctrlKey: false, altKey: false, metaKey: false, shiftKey: false };

/** Камера сверху без перспективы: 10 точек на клетку, начало сцены в (50, 50). */
function ground(x: number, y: number): [number, number] {
  return [(x - 50) / 10, (y - 50) / 10];
}

function pointer(x: number, y: number, extra: Partial<PointerInput> = {}): PointerInput {
  return { pointerId: 1, button: 0, buttons: 1, x, y, ctrlKey: false, shiftKey: false, ...extra };
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
  const moves: [number, number, number][] = [];
  const transforms: [number, Record<string, unknown>][] = [];
  const cameras: EditorCameraState[] = [];
  const fitCalls: unknown[] = [];
  const state = { pickedId: undefined as number | undefined, fitted: START_CAMERA as EditorCameraState | undefined };
  const engine = {
    object_at: () => state.pickedId,
    object_rect: () => ({ corners: [] }),
    ground_at: (x: number, y: number) => ground(x, y),
    screen_point: (x: number, y: number, z: number) => [x * 10 + 50, y * 10 + 50 - z * 10],
    move_object: (id: number, x: number, y: number) => moves.push([id, x, y]),
    transform_object: (id: number, transform: Record<string, unknown>) => transforms.push([id, transform]),
    editor_camera: (camera: EditorCameraState) => cameras.push(camera),
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
  const context: SpaceSceneContext = {
    engine,
    cameraStore,
    isInputLocked: false,
    isEditorCameraActive: true,
    areHandlesAvailable: true,
    selectedIndex: 7,
    selectedLabel: "izba",
    handleMode: "translate",
    getObjectProperties: (id) => worldObjects[id] ?? null,
    onSelect: (index) => selections.push(index),
    onHandleModeChange: (mode) => modes.push(mode),
    onCommitPlacement: (index, changes) => commits.push([index, changes]),
    ...overrides,
  };
  const controller = createSpaceSceneController(() => context);
  return { controller, context, moves, transforms, cameras, fitCalls, commits, selections, modes, state, cameraStore };
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

describe("перенос ручкой", () => {
  it("стрелка оси меняет одну координату, а при отпускании — одно действие с прежним значением", () => {
    const scene = setup();
    expect(scene.controller.pointerDown(pointer(150, 100))).toBe(true);
    scene.controller.pointerMove(pointer(170, 120));
    expect(scene.moves).toEqual([[7, 5, 4]]);
    scene.controller.pointerUp(release(170, 120));
    expect(scene.commits).toEqual([[7, [{ key: "position", value: [5, 4], previous: [3, 4] }]]]);
  });

  it("середина двигает объект свободно до сотой клетки, с Ctrl — до целой", () => {
    const free = setup();
    free.controller.pointerDown(pointer(92, 102));
    free.controller.pointerMove(pointer(95, 113));
    expect(free.moves.at(-1)).toEqual([7, 3.3, 5.1]);

    const snapped = setup();
    snapped.controller.pointerDown(pointer(92, 102));
    snapped.controller.pointerMove(pointer(95, 120, { ctrlKey: true }));
    expect(snapped.moves.at(-1)).toEqual([7, 3, 6]);
  });

  it("Esc отменяет перенос: объект возвращается как был, действия нет", () => {
    const scene = setup();
    scene.controller.pointerDown(pointer(150, 100));
    scene.controller.pointerMove(pointer(170, 120));
    expect(scene.controller.keyDown(key("Escape"))).toBe(true);
    expect(scene.moves.at(-1)).toEqual([7, 3, 4]);
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
    expect(scene.moves).toEqual([[7, 5, 4]]);
    scene.controller.pointerUp(release(120, 100));
    expect(scene.commits).toEqual([[7, [{ key: "position", value: [5, 4], previous: [3, 4] }]]]);
  });

  it("в любом виде ручек нажатие по объекту вне ручек переносит его, а не поворачивает", () => {
    const scene = setup({ handleMode: "rotate" });
    scene.state.pickedId = 7;
    scene.controller.pointerDown(pointer(120, 100));
    scene.controller.pointerMove(pointer(140, 100));
    expect(scene.moves).toEqual([[7, 5, 4]]);
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
    scene.state.fitted = { target: [4, 5], yaw: 0, pitch: 55, distance: 9 };
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
