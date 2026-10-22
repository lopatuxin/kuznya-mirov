import type { EngineError } from "../engineErrors";

/** Русские подписи ключей в тексте ошибки движка: бег и прыжок, езда, модели, виды дорог и перекрёстки, списки экрана, скорость времени, рамки, шкалы, подсказки, шрифты и масштаб интерфейса. Ключ `image` не подписан: так зовутся и картинка объекта, и картинка над ним в мире. */
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
  image_scale: "толщина рамки",
  back_color: "цвет подложки",
  back_image: "картинка подложки",
  tooltip: "подсказка",
  ui_reference: "размер окна образца",
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

/** Подписи ключей, которые бывают и свойствами самой игры (`weight` у груза, `width`, `gap`), — только в ошибках
 * своего места: настроек шрифта, блока вида подсказок в начале screens.json, описаний картинок. Место — начало пути:
 * текст подсказки элемента (`… → elements → 3 → tooltip`) с подстановкой `{delay}` блоком вида не считается. */
const SCOPED_KEY_LABELS: readonly { place: string; labels: Readonly<Record<string, string>> }[] = [
  { place: "files → fonts", labels: { width: "ширина", weight: "толщина", features: "признаки шрифта" } },
  { place: "tooltip", labels: { padding: "отступы", gap: "промежуток", delay: "задержка", font_size: "размер шрифта" } },
  { place: "files → images", labels: { slice: "поля рамки" } },
];

function withScopedLabels(message: string, path: string): string {
  return SCOPED_KEY_LABELS.filter(({ place }) => path === place || path.startsWith(`${place} → `)).reduce(
    (text, { labels }) => text.replace(new RegExp(`"(${Object.keys(labels).join("|")})"`, "g"), (_quoted, key: string) => `"${labels[key]}"`),
    message,
  );
}

/** Ошибка движка с русскими подписями вместо ключей в кавычках и `files.models` в тексте; место в файле (`path`) остаётся как есть — по нему ищут строку. */
export function withErrorLabels(error: EngineError): EngineError {
  const aboutJunctions = error.path.includes(JUNCTIONS_KEY) || error.message.includes(JUNCTIONS_KEY);
  const junctionLabelled = aboutJunctions ? error.message.replace(JUNCTION_KEY_PATTERN, (_quoted, key: string) => `"${JUNCTION_KEY_LABELS[key]}"`) : error.message;
  const message = withScopedLabels(junctionLabelled, error.path);
  return {
    ...error,
    message: message.replace(MODELS_KEY_PATTERN, MODELS_KEY_LABEL).replace(QUOTED_KEY_PATTERN, (_quoted, key: string) => `"${ERROR_KEY_LABELS[key]}"`),
  };
}
