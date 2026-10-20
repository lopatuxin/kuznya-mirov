import type { EngineError } from "../engineErrors";

/** Русские подписи ключей в тексте ошибки движка: бег и прыжок, езда, модели, виды дорог и перекрёстки, списки экрана, скорость времени. Ключ `image` не подписан: так зовутся и картинка объекта, и картинка над ним в мире. */
const ERROR_KEY_LABELS: Readonly<Record<string, string>> = {
  run_speed: "скорость бега",
  jump_height: "высота прыжка",
  jump_time: "время подъёма",
  on_ground: "стоит на земле",
  road: "дорога",
  road_speed: "скорость на дороге",
  drive_speed: "скорость езды",
  drive_to: "куда ехать",
  model: "модель",
  road_kind: "вид дороги",
  road_kinds: "вид дорог",
  piece: "прямой кусок",
  road_junctions: "перекрёстки",
  alphaCutoff: "порог прозрачности",
  height: "высота",
  row_height: "высота строки",
  sort_by: "порядок по",
  time_scale: "скорость времени",
};

/** Ключ `files.models` движок называет с родителем: «files.models есть только в трёхмерной сцене». */
const MODELS_KEY_PATTERN = /\bfiles\.models\b/g;
const MODELS_KEY_LABEL = "ключ models";

/** Ключ в кавычках: движок называет свойство так — «свойство "model" …», «признак "on_ground" …», — и подпись встаёт в фразу, не ломая род; `"model_sizes"`, `"view_height"` и `"models"` не задеваются. */
const QUOTED_KEY_PATTERN = new RegExp(`"(${Object.keys(ERROR_KEY_LABELS).join("|")})"`, "g");

/** Подписи ключей рукава перекрёстка — только в ошибках про `scene.road_junctions`: слова `fade` и `reach` бывают и свойствами самой игры. */
const JUNCTION_KEY_LABELS: Readonly<Record<string, string>> = {
  arms: "рукава",
  reach: "устье рукава",
  fade: "гаснущая полоса",
};
const JUNCTION_KEY_PATTERN = new RegExp(`"(${Object.keys(JUNCTION_KEY_LABELS).join("|")})"`, "g");
const JUNCTIONS_KEY = "road_junctions";

/** Ошибка движка с русскими подписями вместо ключей в кавычках и `files.models` в тексте; место в файле (`path`) остаётся как есть — по нему ищут строку. */
export function withErrorLabels(error: EngineError): EngineError {
  const aboutJunctions = error.path.includes(JUNCTIONS_KEY) || error.message.includes(JUNCTIONS_KEY);
  const message = aboutJunctions ? error.message.replace(JUNCTION_KEY_PATTERN, (_quoted, key: string) => `"${JUNCTION_KEY_LABELS[key]}"`) : error.message;
  return {
    ...error,
    message: message.replace(MODELS_KEY_PATTERN, MODELS_KEY_LABEL).replace(QUOTED_KEY_PATTERN, (_quoted, key: string) => `"${ERROR_KEY_LABELS[key]}"`),
  };
}
