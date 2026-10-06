import { describe, expect, it, vi } from "vitest";
import {
  createEditorCameraStore,
  createFlatCameraStore,
  focusCameraOnObject,
  focusFlatCameraOnObject,
  maxDistanceFor,
  maxViewHeightFor,
  orbitCamera,
  panCamera,
  panFlatCamera,
  readFlatCamera,
  wheelClicks,
  zoomCamera,
  zoomFlatCamera,
  type EditorCameraEngine,
  type EditorCameraRequest,
  type EditorCameraState,
  type FlatCameraEngine,
  type FlatCameraState,
} from "./editorCamera";

const CAMERA: EditorCameraState = { target: [16, 12, 0], yaw: 0, pitch: 55, distance: 40 };

/** `terrainHeight` — высота рельефа под точкой вращения: её движок берёт, когда `target` из двух чисел. */
function fakeEngine(
  fitted: EditorCameraState | undefined,
  terrainHeight = 0,
): { engine: EditorCameraEngine; sent: EditorCameraRequest[]; fitCalls: unknown[] } {
  const sent: EditorCameraRequest[] = [];
  const fitCalls: unknown[] = [];
  const engine = {
    editor_camera: (camera: EditorCameraRequest) => {
      sent.push(camera);
      return camera.target[2] ?? terrainHeight;
    },
    fit_camera: (id?: number | null) => {
      fitCalls.push(id);
      return fitted;
    },
  } as unknown as EditorCameraEngine;
  return { engine, sent, fitCalls };
}

describe("orbitCamera", () => {
  it("ведёшь мышь вправо — yaw уменьшается на 0,4° за точку, вниз — pitch растёт", () => {
    const turned = orbitCamera({ ...CAMERA, yaw: 100 }, 50, 0);
    expect(turned.yaw).toBeCloseTo(80);
    expect(orbitCamera(CAMERA, 0, 25).pitch).toBeCloseTo(65);
  });

  it("pitch остаётся в пределах от 5 до 90 — ниже земли камера не опускается", () => {
    expect(orbitCamera(CAMERA, 0, 1000).pitch).toBe(90);
    expect(orbitCamera(CAMERA, 0, -1000).pitch).toBe(5);
  });

  it("yaw хранится в отрезке от 0 до 360, 360 не включая", () => {
    expect(orbitCamera({ ...CAMERA, yaw: 10 }, 50, 0).yaw).toBeCloseTo(350);
    expect(orbitCamera({ ...CAMERA, yaw: 350 }, -50, 0).yaw).toBeCloseTo(10);
    const full = orbitCamera({ ...CAMERA, yaw: 0 }, 900, 0).yaw;
    expect(full).toBeGreaterThanOrEqual(0);
    expect(full).toBeLessThan(360);
  });

  it("точка и расстояние не меняются", () => {
    const turned = orbitCamera(CAMERA, 30, 30);
    expect(turned.target).toEqual(CAMERA.target);
    expect(turned.distance).toBe(CAMERA.distance);
  });
});

describe("panCamera", () => {
  it("сдвигает точку камеры на разницу мест земли — место, за которое взялась мышь, встаёт под указатель", () => {
    expect(panCamera(CAMERA, [10, 8], [12, 7]).target).toEqual([14, 13]);
  });
});

describe("zoomCamera", () => {
  it("щелчок колеса — расстояние × 0,9 или ÷ 0,9", () => {
    expect(zoomCamera(CAMERA, -1, 120).distance).toBeCloseTo(36);
    expect(zoomCamera(CAMERA, 1, 120).distance).toBeCloseTo(40 / 0.9);
  });

  it("расстояние — от 2 клеток до максимума", () => {
    expect(zoomCamera({ ...CAMERA, distance: 2.1 }, -5, 120).distance).toBe(2);
    expect(zoomCamera({ ...CAMERA, distance: 119 }, 5, 120).distance).toBe(120);
  });

  it("максимум — три расстояния, с которых видна вся земля", () => {
    expect(maxDistanceFor(40)).toBe(120);
  });
});

describe("wheelClicks", () => {
  it("щелчок колеса в пикселях — сотня, меньшая прокрутка тоже считается щелчком, знак сохраняется", () => {
    expect(wheelClicks(100, 0)).toBe(1);
    expect(wheelClicks(-100, 0)).toBe(-1);
    expect(wheelClicks(4, 0)).toBe(1);
    expect(wheelClicks(-250, 0)).toBe(-2.5);
  });

  it("строки и страницы сводятся к щелчкам", () => {
    expect(wheelClicks(3, 1)).toBe(1);
    expect(wheelClicks(-1, 2)).toBe(-1);
  });
});

describe("createEditorCameraStore", () => {
  it("первая загрузка берёт камеру на всю землю у движка, ставит её и запоминает три расстояния", () => {
    const store = createEditorCameraStore();
    const { engine, sent, fitCalls } = fakeEngine(CAMERA);
    store.restore(engine);
    expect(fitCalls).toEqual([null]);
    expect(sent).toEqual([CAMERA]);
    expect(store.current()).toEqual({ camera: CAMERA, maxDistance: 120 });
  });

  it("следующая загрузка возвращает движку прежнюю камеру, не спрашивая новую", () => {
    const store = createEditorCameraStore();
    const first = fakeEngine(CAMERA);
    store.restore(first.engine);
    const turned = { ...CAMERA, yaw: 90 };
    store.update(first.engine, turned);

    const reloaded = fakeEngine({ ...CAMERA, distance: 1 });
    store.restore(reloaded.engine);
    expect(reloaded.fitCalls).toEqual([]);
    expect(reloaded.sent).toEqual([turned]);
  });

  it("плоская сцена камеры не даёт: ничего не ставится, а следующая успешная загрузка ставит", () => {
    const store = createEditorCameraStore();
    const flat = fakeEngine(undefined);
    store.restore(flat.engine);
    expect(flat.sent).toEqual([]);
    expect(store.current()).toBe(null);

    const spatial = fakeEngine(CAMERA);
    store.restore(spatial.engine);
    expect(spatial.sent).toEqual([CAMERA]);
  });

  it("холст получил окончательный размер — нетронутая камера подбирается заново под него, пределы тоже", () => {
    const store = createEditorCameraStore();
    store.restore(fakeEngine({ ...CAMERA, distance: 20 }).engine);
    const resized = fakeEngine(CAMERA);
    store.refit(resized.engine);
    expect(resized.fitCalls).toEqual([null]);
    expect(resized.sent).toEqual([CAMERA]);
    expect(store.current()).toEqual({ camera: CAMERA, maxDistance: 120 });
  });

  it("камеру, которую водили мышью или подвели клавишей F, изменение размера не трогает", () => {
    const store = createEditorCameraStore();
    const first = fakeEngine(CAMERA);
    store.restore(first.engine);
    const turned = { ...CAMERA, yaw: 90 };
    store.update(first.engine, turned);

    const resized = fakeEngine({ ...CAMERA, distance: 1 });
    store.refit(resized.engine);
    expect(resized.fitCalls).toEqual([]);
    expect(resized.sent).toEqual([]);
    expect(store.current()?.camera).toEqual(turned);
  });

  it("до первой успешной загрузки и в плоской сцене подбирать нечего", () => {
    const store = createEditorCameraStore();
    const before = fakeEngine(CAMERA);
    store.refit(before.engine);
    expect(before.fitCalls).toEqual([]);

    store.restore(fakeEngine(CAMERA).engine);
    const flat = fakeEngine(undefined);
    store.refit(flat.engine);
    expect(flat.sent).toEqual([]);
    expect(store.current()?.camera).toEqual(CAMERA);
  });

  it("сброс забывает камеру и пределы", () => {
    const store = createEditorCameraStore();
    store.restore(fakeEngine(CAMERA).engine);
    store.reset();
    expect(store.current()).toBe(null);
  });
});

describe("высота точки вращения", () => {
  it("вращение и колесо шлют высоту точки ровно, какой она была", () => {
    const store = createEditorCameraStore();
    const { engine, sent } = fakeEngine({ ...CAMERA, target: [16, 12, 3.4] }, 9);
    store.restore(engine);
    sent.length = 0;

    store.update(engine, orbitCamera(store.current()?.camera as EditorCameraState, 30, 10));
    store.update(engine, zoomCamera(store.current()?.camera as EditorCameraState, -1, 120));

    expect(sent.map((camera) => camera.target)).toEqual([[16, 12, 3.4], [16, 12, 3.4]]);
    expect(store.current()?.camera.target).toEqual([16, 12, 3.4]);
  });

  it("сдвиг Shift шлёт два числа — высоту берёт рельеф под новой точкой, и страница её запоминает", () => {
    const store = createEditorCameraStore();
    const { engine, sent } = fakeEngine(CAMERA, 1.5);
    store.restore(engine);
    sent.length = 0;

    store.update(engine, panCamera(store.current()?.camera as EditorCameraState, [10, 8], [12, 7]));

    expect(sent[0]?.target).toEqual([14, 13]);
    expect(store.current()?.camera.target).toEqual([14, 13, 1.5]);
  });

  it("перезагрузка файлов возвращает движку ту же высоту точки, даже если рельеф под ней сменился", () => {
    const store = createEditorCameraStore();
    const { engine } = fakeEngine({ ...CAMERA, target: [16, 12, 3.4] });
    store.restore(engine);

    const reloaded = fakeEngine(undefined, 0);
    store.restore(reloaded.engine);

    expect(reloaded.sent[0]?.target).toEqual([16, 12, 3.4]);
  });
});

describe("focusCameraOnObject", () => {
  it("ставит камеру, что вернул движок, и запоминает её", () => {
    const store = createEditorCameraStore();
    const { engine, sent, fitCalls } = fakeEngine(CAMERA);
    store.restore(engine);
    fitCalls.length = 0;
    sent.length = 0;

    const objectCamera: EditorCameraState = { target: [6, 13, 0.7], yaw: 20, pitch: 40, distance: 9 };
    const focusEngine = fakeEngine(objectCamera, 2.5);
    focusCameraOnObject(store, focusEngine.engine, 12);
    expect(focusEngine.fitCalls).toEqual([12]);
    expect(focusEngine.sent).toEqual([{ ...objectCamera, target: [6, 13] }]);
    expect(store.current()?.camera).toEqual({ ...objectCamera, target: [6, 13, 2.5] });
  });

  it("расстояние не меньше двух клеток — у маленького объекта ставится 2", () => {
    const store = createEditorCameraStore();
    store.restore(fakeEngine(CAMERA).engine);
    const focusEngine = fakeEngine({ ...CAMERA, distance: 0.7 });
    focusCameraOnObject(store, focusEngine.engine, 3);
    expect(focusEngine.sent[0]?.distance).toBe(2);
  });

  it("у объекта без места движок ничего не вернул — камера не меняется", () => {
    const store = createEditorCameraStore();
    store.restore(fakeEngine(CAMERA).engine);
    const update = vi.fn();
    focusCameraOnObject(store, { editor_camera: update, fit_camera: () => undefined } as unknown as EditorCameraEngine, 3);
    expect(update).not.toHaveBeenCalled();
    expect(store.current()?.camera).toEqual(CAMERA);
  });
});

const FLAT_WHOLE_SCENE: FlatCameraState = { center: [80, 12], view_height: 80 };

function fakeFlatEngine(fitted: unknown): { engine: FlatCameraEngine; sent: FlatCameraState[]; fitCalls: unknown[] } {
  const sent: FlatCameraState[] = [];
  const fitCalls: unknown[] = [];
  const engine = {
    editor_camera: (camera: FlatCameraState) => {
      sent.push(camera);
    },
    fit_camera: (id?: number | null) => {
      fitCalls.push(id);
      return fitted;
    },
  } as unknown as FlatCameraEngine;
  return { engine, sent, fitCalls };
}

describe("zoomFlatCamera", () => {
  it("щелчок на себя над точкой (40, 12): высота 72, середина (76, 12) — пример требования 6", () => {
    const zoomed = zoomFlatCamera(FLAT_WHOLE_SCENE, -1, [40, 12], 240);

    expect(zoomed.view_height).toBeCloseTo(72);
    expect(zoomed.center[0]).toBeCloseTo(76);
    expect(zoomed.center[1]).toBeCloseTo(12);
  });

  it("щелчок от себя — высота ÷ 0,9, точка под указателем остаётся на месте", () => {
    const zoomed = zoomFlatCamera(FLAT_WHOLE_SCENE, 1, [40, 12], 240);

    expect(zoomed.view_height).toBeCloseTo(80 / 0.9);
    const ratio = zoomed.view_height / 80;
    expect(40 - (40 - zoomed.center[0]) / ratio).toBeCloseTo(80);
  });

  it("высота — от 2 клеток до максимума", () => {
    expect(zoomFlatCamera(FLAT_WHOLE_SCENE, -100, [40, 12], 240).view_height).toBe(2);
    expect(zoomFlatCamera(FLAT_WHOLE_SCENE, 100, [40, 12], 240).view_height).toBe(240);
  });

  it("у предела точка под указателем тоже остаётся под ним", () => {
    const zoomed = zoomFlatCamera({ center: [80, 12], view_height: 2.1 }, -5, [78, 11], 240);

    expect(zoomed.view_height).toBe(2);
    expect(zoomed.center[0]).toBeCloseTo(78 + (80 - 78) * (2 / 2.1));
  });
});

describe("maxViewHeightFor", () => {
  it("три высоты, с которых видна вся сцена; не меньше 2 клеток", () => {
    expect(maxViewHeightFor(80)).toBe(240);
    expect(maxViewHeightFor(0.1)).toBe(2);
  });
});

describe("panFlatCamera", () => {
  it("сдвигает середину на разницу мест сцены — взятое место встаёт под указатель", () => {
    expect(panFlatCamera(FLAT_WHOLE_SCENE, [10, 8], [12, 7])).toEqual({ center: [78, 13], view_height: 80 });
  });
});

describe("readFlatCamera", () => {
  it("принимает только ответ плоской сцены", () => {
    expect(readFlatCamera(FLAT_WHOLE_SCENE)).toBe(FLAT_WHOLE_SCENE);
    expect(readFlatCamera(CAMERA)).toBeUndefined();
    expect(readFlatCamera(undefined)).toBeUndefined();
  });
});

describe("createFlatCameraStore", () => {
  it("до первой загрузки камеры нет, update ничего не шлёт", () => {
    const { engine, sent } = fakeFlatEngine(FLAT_WHOLE_SCENE);
    const store = createFlatCameraStore();

    store.update(engine, { center: [1, 1], view_height: 5 });

    expect(store.current()).toBeNull();
    expect(sent).toEqual([]);
  });

  it("первая загрузка берёт у движка камеру на всю сцену и шлёт её; дальше возвращается прежняя", () => {
    const { engine, sent, fitCalls } = fakeFlatEngine(FLAT_WHOLE_SCENE);
    const store = createFlatCameraStore();

    store.restore(engine);
    store.update(engine, { center: [76, 12], view_height: 72 });
    store.restore(engine);

    expect(fitCalls).toEqual([null]);
    expect(sent).toEqual([FLAT_WHOLE_SCENE, { center: [76, 12], view_height: 72 }, { center: [76, 12], view_height: 72 }]);
    expect(store.current()).toEqual({ center: [76, 12], view_height: 72 });
  });

  it("новый размер холста подбирает камеру заново, пока её не трогали; после колеса — нет", () => {
    const { engine, sent, fitCalls } = fakeFlatEngine(FLAT_WHOLE_SCENE);
    const store = createFlatCameraStore();
    store.restore(engine);

    store.refit(engine);
    expect(fitCalls).toEqual([null, null]);

    store.update(engine, { center: [76, 12], view_height: 72 });
    sent.length = 0;
    store.refit(engine);
    expect(sent).toEqual([]);
    expect(fitCalls).toHaveLength(2);
  });

  it("ответ трёхмерной сцены камерой не становится", () => {
    const { engine, sent } = fakeFlatEngine(CAMERA);
    const store = createFlatCameraStore();

    store.restore(engine);

    expect(store.current()).toBeNull();
    expect(sent).toEqual([]);
  });

  it("другой проект камеру забывает", () => {
    const { engine } = fakeFlatEngine(FLAT_WHOLE_SCENE);
    const store = createFlatCameraStore();
    store.restore(engine);

    store.reset();

    expect(store.current()).toBeNull();
  });
});

describe("focusFlatCameraOnObject", () => {
  it("ставит камеру, что подвёл движок", () => {
    const { engine, sent, fitCalls } = fakeFlatEngine({ center: [168, 26], view_height: 14.4 });
    const store = createFlatCameraStore();
    store.restore(engine);
    fitCalls.length = 0;
    sent.length = 0;

    focusFlatCameraOnObject(store, engine, 4);

    expect(fitCalls).toEqual([4]);
    expect(sent).toEqual([{ center: [168, 26], view_height: 14.4 }]);
  });

  it("движок объекта не знает — камера прежняя", () => {
    const { engine } = fakeFlatEngine(FLAT_WHOLE_SCENE);
    const store = createFlatCameraStore();
    store.restore(engine);
    const missing = fakeFlatEngine(undefined);

    focusFlatCameraOnObject(store, missing.engine, 4);

    expect(missing.sent).toEqual([]);
    expect(store.current()).toEqual(FLAT_WHOLE_SCENE);
  });
});
