import type { EngineEditResult } from "./battleTypes";

/** Ровный ветер сцены — `wind` корня `scene.json`: клеток в секунду, `x` вправо, `y` вниз. */
export type SceneWind = readonly [number, number];

export const NO_WIND: SceneWind = [0, 0];

/**
 * Ветер из текста `scene.json` — «Ветер и частицы»: нет ключа или он не пара конечных чисел — ветра нет;
 * такую сцену движок всё равно не загрузит, и кнопка «Ветер» при ошибках проекта неактивна.
 */
export function parseSceneWind(sceneText: string | null): SceneWind {
  if (sceneText === null) return NO_WIND;
  let parsed: unknown;
  try {
    parsed = JSON.parse(sceneText);
  } catch {
    return NO_WIND;
  }
  const wind = (parsed as { wind?: unknown } | null)?.wind;
  if (!Array.isArray(wind) || wind.length !== 2) return NO_WIND;
  const [x, y] = wind as unknown[];
  return typeof x === "number" && Number.isFinite(x) && typeof y === "number" && Number.isFinite(y) ? [x, y] : NO_WIND;
}

/** Вызов движка, которым ветер проверяется и ставится миру — «Редактор», «Вызовы движка». */
export type WindEditor = { set_wind(wind: unknown): unknown };

/** Вызов движка, который отдаёт ровный ветер мира сейчас: живой в партии и повторе, ветер файла вне них. */
export type WindReader = { wind(): unknown };

/** Ветер, который действует в мире движка сейчас — единственный источник для полей окошка «Ветер» в партии и повторе. */
export function readEngineWind(engine: WindReader): SceneWind {
  const [x, y] = engine.wind() as [number, number];
  return [x, y];
}

/** Ставит ветер миру движка; текст ошибки по-русски, если движок его не принял, иначе `undefined`. */
export function applyWind(editor: WindEditor, wind: SceneWind): string | undefined {
  const result = editor.set_wind([wind[0], wind[1]]) as EngineEditResult;
  return result.ok ? undefined : result.error;
}
