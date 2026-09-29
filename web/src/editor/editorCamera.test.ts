import { describe, expect, it, vi } from "vitest";
import {
  createEditorCameraStore,
  focusCameraOnObject,
  maxDistanceFor,
  orbitCamera,
  panCamera,
  wheelClicks,
  zoomCamera,
  type EditorCameraEngine,
  type EditorCameraState,
} from "./editorCamera";

const CAMERA: EditorCameraState = { target: [16, 12], yaw: 0, pitch: 55, distance: 40 };

function fakeEngine(fitted: EditorCameraState | undefined): { engine: EditorCameraEngine; sent: EditorCameraState[]; fitCalls: unknown[] } {
  const sent: EditorCameraState[] = [];
  const fitCalls: unknown[] = [];
  const engine = {
    editor_camera: (camera: EditorCameraState) => sent.push(camera),
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

describe("focusCameraOnObject", () => {
  it("ставит камеру, что вернул движок, и запоминает её", () => {
    const store = createEditorCameraStore();
    const { engine, sent, fitCalls } = fakeEngine(CAMERA);
    store.restore(engine);
    fitCalls.length = 0;
    sent.length = 0;

    const objectCamera = { target: [6, 13] as [number, number], yaw: 20, pitch: 40, distance: 9 };
    const focusEngine = fakeEngine(objectCamera);
    focusCameraOnObject(store, focusEngine.engine, 12);
    expect(focusEngine.fitCalls).toEqual([12]);
    expect(focusEngine.sent).toEqual([objectCamera]);
    expect(store.current()?.camera).toEqual(objectCamera);
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
