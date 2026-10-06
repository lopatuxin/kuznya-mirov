import type { Engine } from "engine";
import type { Vec2 } from "./objectPlacement";

/** То, что возвращает `fit_camera`; `target` — точка вращения в середине окна: место на земле и её высота. */
export type EditorCameraState = { target: [number, number, number]; yaw: number; pitch: number; distance: number };

/**
 * То, что шлёт `editor_camera`: `target` из двух чисел — высоту точки вращения движок берёт из рельефа
 * под ней (автор сам ведёт камеру), из трёх — ровно такую («Кисти рельефа», требование 21).
 */
export type EditorCameraRequest = Omit<EditorCameraState, "target"> & { target: [number, number] | [number, number, number] };

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
 * вместе со своей точкой на разницу, и `grabbed` встаёт под указатель; высота точки — из рельефа под ней.
 */
export function panCamera(camera: EditorCameraState, grabbed: Vec2, under: Vec2): EditorCameraRequest {
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
  /** Ставит камеру и шлёт её движку; высоту точки вращения, которую тот взял, запоминает. */
  update(engine: EditorCameraEngine, camera: EditorCameraRequest): void;
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
    // В плоской сцене `fit_camera` отвечает `{center, view_height}` — это камера другого хранилища.
    if (fitted === undefined || !("target" in fitted)) return;
    state = { camera: fitted, maxDistance: maxDistanceFor(fitted.distance) };
    isGroundFit = true;
    engine.editor_camera(fitted);
  }

  return {
    current: () => state,
    update(engine, camera) {
      if (state === null) return;
      const height = engine.editor_camera(camera);
      const target: [number, number, number] = [camera.target[0], camera.target[1], height ?? camera.target[2] ?? state.camera.target[2]];
      state = { ...state, camera: { ...camera, target } };
      isGroundFit = false;
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
 * движок по последней `editor_camera`), расстояние не меньше двух клеток, высоту точки вращения берёт рельеф под ней («Кисти рельефа», требование 21). Без объекта, его `position`
 * и `size` и в плоской сцене ничего не меняется.
 */
export function focusCameraOnObject(store: EditorCameraStore, engine: EditorCameraEngine, objectId: number): void {
  const fitted = engine.fit_camera(objectId) as EditorCameraState | undefined;
  if (fitted === undefined) return;
  store.update(engine, { ...fitted, target: [fitted.target[0], fitted.target[1]], distance: Math.max(MIN_DISTANCE, fitted.distance) });
}

/** То, что шлёт `editor_camera` и возвращает `fit_camera` в плоской сцене: середина видимой части и сколько клеток видно по высоте. */
export type FlatCameraState = { center: [number, number]; view_height: number };

/** Вызовы движка, которыми пользуется камера редактора плоской сцены. */
export type FlatCameraEngine = Pick<Engine, "editor_camera" | "fit_camera">;

/**
 * Колесо плоской сцены — «Редактор», «Сцена»: щелчок — `view_height` × 0,9 или ÷ 0,9, от 2 клеток до
 * `maxViewHeight`; `under` — место сцены под указателем, оно остаётся под ним: середина отходит от него
 * во столько же раз, во сколько меняется высота.
 */
export function zoomFlatCamera(camera: FlatCameraState, clicks: number, under: Vec2, maxViewHeight: number): FlatCameraState {
  const viewHeight = Math.min(maxViewHeight, Math.max(MIN_DISTANCE, camera.view_height * WHEEL_STEP ** -clicks));
  const ratio = viewHeight / camera.view_height;
  return { center: [under[0] - (under[0] - camera.center[0]) * ratio, under[1] - (under[1] - camera.center[1]) * ratio], view_height: viewHeight };
}

/**
 * Сдвиг средней кнопкой — «Редактор», «Сцена»: `grabbed` — место сцены, за которое взялась мышь, `under` — место
 * под указателем сейчас при нынешней камере; середина сдвигается на разницу, и `grabbed` встаёт под указатель.
 */
export function panFlatCamera(camera: FlatCameraState, grabbed: Vec2, under: Vec2): FlatCameraState {
  return { ...camera, center: [camera.center[0] + grabbed[0] - under[0], camera.center[1] + grabbed[1] - under[1]] };
}

/** Самая большая высота вида — три высоты, с которых видна вся сцена в окне сейчас. */
export function maxViewHeightFor(wholeSceneViewHeight: number): number {
  return Math.max(MIN_DISTANCE, wholeSceneViewHeight * MAX_DISTANCE_FACTOR);
}

/**
 * Камера редактора плоской сцены, которую держит редактор, — как у трёхмерной: движок помнит её сам, но
 * новая загрузка игры её забывает, поэтому редактор шлёт её после каждого `show_scene`.
 */
export type FlatCameraStore = {
  /** Камера; `null` — ещё не было успешной загрузки плоской сцены. */
  current(): FlatCameraState | null;
  /** Ставит камеру и шлёт её движку. */
  update(engine: FlatCameraEngine, camera: FlatCameraState): void;
  /** После `show_scene`: первая загрузка берёт у движка камеру на всю сцену, дальше — прежняя камера. */
  restore(engine: FlatCameraEngine): void;
  /** Холст получил новый размер: камера, которую никто не трогал, снова подбирается на всю сцену под это окно. */
  refit(engine: FlatCameraEngine): void;
  /** Проект сменился — камера забывается. */
  reset(): void;
};

/** Ответ плоского `fit_camera`; трёхмерный ответ и `undefined` — `undefined`. */
export function readFlatCamera(fitted: unknown): FlatCameraState | undefined {
  return fitted !== null && typeof fitted === "object" && "center" in fitted && "view_height" in fitted ? (fitted as FlatCameraState) : undefined;
}

export function createFlatCameraStore(): FlatCameraStore {
  let state: FlatCameraState | null = null;
  let isWholeSceneFit = false;

  function fitWholeScene(engine: FlatCameraEngine): void {
    const fitted = readFlatCamera(engine.fit_camera(null));
    if (fitted === undefined) return;
    state = fitted;
    isWholeSceneFit = true;
    engine.editor_camera(fitted);
  }

  return {
    current: () => state,
    update(engine, camera) {
      if (state === null) return;
      engine.editor_camera(camera);
      state = camera;
      isWholeSceneFit = false;
    },
    restore(engine) {
      if (state === null) {
        fitWholeScene(engine);
        return;
      }
      engine.editor_camera(state);
    },
    refit(engine) {
      if (state !== null && isWholeSceneFit) fitWholeScene(engine);
    },
    reset() {
      state = null;
      isWholeSceneFit = false;
    },
  };
}

/** `F` в плоской сцене — «Редактор», требование 7: камера подходит к месту, где объект нарисован. Без объекта, его `position` и `size` ничего не меняется. */
export function focusFlatCameraOnObject(store: FlatCameraStore, engine: FlatCameraEngine, objectId: number): void {
  const fitted = readFlatCamera(engine.fit_camera(objectId));
  if (fitted !== undefined) store.update(engine, fitted);
}
