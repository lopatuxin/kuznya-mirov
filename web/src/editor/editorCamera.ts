import type { Engine } from "engine";
import type { Vec2 } from "./objectPlacement";

/** То, что шлёт `editor_camera` и возвращает `fit_camera`; `target` — точка на земле в середине окна. */
export type EditorCameraState = { target: [number, number]; yaw: number; pitch: number; distance: number };

/** Вызовы движка, которыми пользуется камера редактора. */
export type EditorCameraEngine = Pick<Engine, "editor_camera" | "fit_camera">;

const PITCH_MIN = 5;
const PITCH_MAX = 90;
const DEGREES_PER_POINT = 0.4;
const MIN_DISTANCE = 2;
const MAX_DISTANCE_FACTOR = 3;
const WHEEL_STEP = 0.9;
const WHEEL_PIXELS_PER_CLICK = 100;
const WHEEL_LINES_PER_CLICK = 3;
const WHEEL_DELTA_LINES = 1;
const WHEEL_DELTA_PAGES = 2;

/** `yaw` в отрезке от 0 до 360, 360 не включая. */
function normalizeYaw(degrees: number): number {
  return ((degrees % 360) + 360) % 360;
}

/**
 * Вращение средней кнопкой — «Редактор», требование 2: сцена идёт за мышью, как будто её крутят
 * рукой, поэтому ведёшь вправо — `yaw` уменьшается; ведёшь вниз — камера поднимается, `pitch` растёт.
 */
export function orbitCamera(camera: EditorCameraState, deltaX: number, deltaY: number): EditorCameraState {
  return {
    ...camera,
    yaw: normalizeYaw(camera.yaw - deltaX * DEGREES_PER_POINT),
    pitch: Math.min(PITCH_MAX, Math.max(PITCH_MIN, camera.pitch + deltaY * DEGREES_PER_POINT)),
  };
}

/**
 * Сдвиг Shift + средней кнопкой — «Редактор», требование 3: `grabbed` — место земли, за которое
 * взялась мышь, `under` — место земли под указателем сейчас при нынешней камере. Камера сдвигается
 * вместе со своей точкой на разницу, и `grabbed` встаёт под указатель.
 */
export function panCamera(camera: EditorCameraState, grabbed: Vec2, under: Vec2): EditorCameraState {
  return { ...camera, target: [camera.target[0] + grabbed[0] - under[0], camera.target[1] + grabbed[1] - under[1]] };
}

/** Самое дальнее расстояние камеры — три расстояния, с которых видна вся земля при открытии проекта. */
export function maxDistanceFor(groundFitDistance: number): number {
  return Math.max(MIN_DISTANCE, groundFitDistance * MAX_DISTANCE_FACTOR);
}

/** Щелчки колеса: пиксели, строки и страницы сводятся к числу щелчков; положительное — от себя, то есть отдаление. */
export function wheelClicks(deltaY: number, deltaMode: number): number {
  let perClick = WHEEL_PIXELS_PER_CLICK;
  if (deltaMode === WHEEL_DELTA_LINES) perClick = WHEEL_LINES_PER_CLICK;
  if (deltaMode === WHEEL_DELTA_PAGES) perClick = 1;
  const clicks = deltaY / perClick;
  return Math.sign(clicks) * Math.max(1, Math.abs(clicks));
}

/** Колесо — «Редактор», требование 3: щелчок — расстояние × 0,9 или ÷ 0,9, от 2 клеток до `maxDistance`. */
export function zoomCamera(camera: EditorCameraState, clicks: number, maxDistance: number): EditorCameraState {
  const distance = camera.distance / WHEEL_STEP ** clicks;
  return { ...camera, distance: Math.min(maxDistance, Math.max(MIN_DISTANCE, distance)) };
}

/**
 * Камера редактора, которую держит редактор, — «Технические детали»: движок помнит её сам, но
 * перезагрузка проекта собирает игру заново, поэтому редактор шлёт её после каждого `show_scene`.
 * До первой успешной загрузки камеры нет.
 */
export type EditorCameraStore = {
  /** Камера и самое дальнее расстояние; `null` — ещё не было успешной загрузки трёхмерной сцены. */
  current(): { camera: EditorCameraState; maxDistance: number } | null;
  /** Ставит камеру и шлёт её движку. */
  update(engine: EditorCameraEngine, camera: EditorCameraState): void;
  /** После `show_scene`: первая загрузка берёт у движка камеру на всю землю, дальше — прежняя камера. */
  restore(engine: EditorCameraEngine): void;
  /** Холст получил новый размер: камера, которую никто не трогал, снова подбирается на всю землю под это окно. */
  refit(engine: EditorCameraEngine): void;
  /** Проект сменился — камера и пределы забываются. */
  reset(): void;
};

export function createEditorCameraStore(): EditorCameraStore {
  let state: { camera: EditorCameraState; maxDistance: number } | null = null;
  let isGroundFit = false;

  function fitGround(engine: EditorCameraEngine): void {
    const fitted = engine.fit_camera(null) as EditorCameraState | undefined;
    if (fitted === undefined) return;
    state = { camera: fitted, maxDistance: maxDistanceFor(fitted.distance) };
    isGroundFit = true;
    engine.editor_camera(fitted);
  }

  return {
    current: () => state,
    update(engine, camera) {
      if (state === null) return;
      state = { ...state, camera };
      isGroundFit = false;
      engine.editor_camera(camera);
    },
    restore(engine) {
      if (state === null) {
        fitGround(engine);
        return;
      }
      engine.editor_camera(state.camera);
    },
    refit(engine) {
      if (state !== null && isGroundFit) fitGround(engine);
    },
    reset() {
      state = null;
      isGroundFit = false;
    },
  };
}

/**
 * `F` — «Редактор», требование 4: камера подходит к объекту, поворот и наклон прежние (их помнит
 * движок по последней `editor_camera`), расстояние не меньше двух клеток. Без объекта, его `position`
 * и `size` и в плоской сцене ничего не меняется.
 */
export function focusCameraOnObject(store: EditorCameraStore, engine: EditorCameraEngine, objectId: number): void {
  const fitted = engine.fit_camera(objectId) as EditorCameraState | undefined;
  if (fitted === undefined) return;
  store.update(engine, { ...fitted, distance: Math.max(MIN_DISTANCE, fitted.distance) });
}
