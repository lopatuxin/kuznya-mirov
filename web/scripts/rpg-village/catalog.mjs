// Каталог кусков деревни («Фаза-02-5-первая-нарисованная-локация», требование 9): для каждого
// куска — исходник (`file`) в `games/rpg/images/` (уже обработан `tools/art/buildVillagePieces.mjs`:
// фон снят, обрезан по содержимому, приведён к масштабу 96 точек на клетку), слой (`layer`), вид
// (`kind` — резать ли на полоски и как ставить), и прямоугольники препятствий относительно куска —
// у стен это лесенка (`obstacleCells`, посчитана один раз здесь же и берётся сборкой сцены отсюда,
// а не пересчитывается из числа столбцов картинки при каждой сборке), у построек/деревьев/камней —
// `footprint`, у ручья — `waterCells` (та же лесенка, но толщиной в два ряда, для «под водой —
// невидимые препятствия», требование 12).
//
// `kind`:
// - `wallStrip` — стена/ограда, идёт по диагонали (требование 2: два вправо на одну вверх), режется
//   на полоски шириной в клетку (требование 13); препятствия — лесенка по той же диагонали,
//   `obstacleCells[i]` — клетка столбца `i` относительно точки постановки (совпадает с низом
//   картинки в этом столбце). `columns` — число столбцов у необрезанного куска `file` (его же
//   пишет `tools/art/`); расположение может попросить меньше («короче целого куска» — требование 1
//   плана, полоска строится из первых `columns` столбцов). `file` лежит вне `games/rpg/images/`
//   (`tools/art/buildVillagePieces.mjs` пишет его во временную папку `tools/art/.build/` —
//   требование 7: промежуточный кусок не должен попасть в образ игры), `output` — куда лечь готовым
//   листом полосок внутри `games/rpg/images/` (его называет `game.json`).
// - `ground` — дорожка или ручей: рисуется одним объектом без `obstacle`, на слое ниже всех
//   (`GROUND_LAYER`, требование 12), концы уже погашены `tools/art/` (требование 7 фазы «Картинки»).
//   У ручья (`stream`, `stream_bridge`) — `waterCells`: невидимые препятствия под водой, у моста —
//   с разрывом там, где на картинке дощатый настил (по картинке моста, разбитой на клетки, — колонки
//   3–7 из 0–10; настил идёт поперёк течения, по зеркальной диагонали).
// - `building` / `foliage` / `post` — один объект: маленький прямоугольник препятствия у основания
//   (`footprint`, в клетках, по центру под картинкой), картинка крупнее с `anchor: "bottom"`
//   (требование 15 — дерево/валун/куст; постройки и столб того же простого вида, «первого уровня»).
//   У `foliage`/`post` — ещё и `targetSize`: до какого размера в клетках приводит картинку
//   `tools/art/buildVillagePieces.mjs` (требование 5 — валун, куст, столб, ель, берёза нарисованы
//   без заготовки, поэтому без неё же и не с чем сверить масштаб; цель задана здесь, в каталоге, а
//   не вычислена из картинки, чтобы при перерисовке куска масштаб не поплыл вместе с ней).

import { diagonalStaircaseCells } from "./wallStrip.mjs";

export const GROUND_LAYER = 0; // требование 12: дорожки и ручей ниже всех объектов
export const MARKER_LAYER = 1; // выше дорожек и ручья (требование 4 фазы — отдельно от их слоя)
export const OBSTACLE_LAYER = 2; // стены, постройки, деревья, герой, враги — участвуют в y_sort

export const KNOWN_KINDS = new Set(["wallStrip", "ground", "building", "foliage", "post"]);

// Лесенка стены: `columns` столбцов картинки, препятствие столбца `i` — клетка `(i, -floor(i/2))`
// относительно точки постановки (реэкспорт формулы `wallStrip.mjs`, чтобы каталог не считал её
// заново на свой лад — «Reuse before writing»).
function wallObstacleCells(columns) {
  return diagonalStaircaseCells(columns);
}

// Невидимая вода: та же лесенка, но в два ряда толщиной (второй ряд — на клетку выше первого, к
// центру потока), на колонках `[0, columns)`, за вычетом `deckColumns` (там, где на картинке настил
// моста, требование 12: «под мостом их нет»).
function waterCells(columns, deckColumns = []) {
  const [deckStart, deckEnd] = deckColumns;
  const cells = [];
  for (const { x, y } of diagonalStaircaseCells(columns)) {
    if (deckColumns.length > 0 && x >= deckStart && x < deckEnd) continue;
    cells.push({ x, y });
    cells.push({ x, y: y - 1 });
  }
  return cells;
}

const STREAM_COLUMNS = 11; // ширина ручья ≈ 10,3 клетки (`stream.png`, 990×534) — 11 колонок с запасом
const BRIDGE_DECK_COLUMNS = [3, 8]; // размечено на глаз по картинке моста, разбитой на клетки, — настил

export const PIECES = {
  wall_log: { file: "wall_log_raw.png", output: "wall_log.png", kind: "wallStrip", layer: OBSTACLE_LAYER, columns: 10, obstacleCells: wallObstacleCells(10) },
  wall_stone: { file: "wall_stone_raw.png", output: "wall_stone.png", kind: "wallStrip", layer: OBSTACLE_LAYER, columns: 10, obstacleCells: wallObstacleCells(10) },
  wall_palisade: { file: "wall_palisade_raw.png", output: "wall_palisade.png", kind: "wallStrip", layer: OBSTACLE_LAYER, columns: 7, obstacleCells: wallObstacleCells(7) },
  // Требование 5: столб чуть выше самой высокой стены — частокол, видимый ряд колышков ≈2,4 клетки
  // (измерено по первому столбцу `wall_palisade_raw.png`, до резки на полоски, — не путать с полной
  // высотой обрезанной картинки: та включает подъём всей диагонали, а не рост одного колышка).
  post: { file: "post.png", kind: "post", layer: OBSTACLE_LAYER, footprint: { width: 0.5, height: 0.5 }, targetSize: { axis: "height", cells: 2.6 } },

  path_straight: { file: "path_straight.png", kind: "ground", layer: GROUND_LAYER },
  path_turn: { file: "path_turn.png", kind: "ground", layer: GROUND_LAYER },
  stream: { file: "stream.png", kind: "ground", layer: GROUND_LAYER, waterCells: waterCells(STREAM_COLUMNS) },
  stream_bridge: { file: "stream_bridge.png", kind: "ground", layer: GROUND_LAYER, waterCells: waterCells(STREAM_COLUMNS, BRIDGE_DECK_COLUMNS) },

  izba: { file: "izba.png", kind: "building", layer: OBSTACLE_LAYER, footprint: { width: 3, height: 2 } },
  smithy: { file: "smithy.png", kind: "building", layer: OBSTACLE_LAYER, footprint: { width: 3.5, height: 2.5 } },
  chertog: { file: "chertog.png", kind: "building", layer: OBSTACLE_LAYER, footprint: { width: 4, height: 2 } },

  // Требование 5: цель по герою (≈1,5 клетки ростом) и образцу — ель и берёза в 2–3 его роста, валун
  // около 2 клеток в ширину, куст около 1,5.
  spruce: { file: "spruce.png", kind: "foliage", layer: OBSTACLE_LAYER, footprint: { width: 0.8, height: 0.5 }, targetSize: { axis: "height", cells: 4.2 } },
  birch: { file: "birch.png", kind: "foliage", layer: OBSTACLE_LAYER, footprint: { width: 0.7, height: 0.5 }, targetSize: { axis: "height", cells: 3.8 } },
  boulder: { file: "boulder.png", kind: "foliage", layer: OBSTACLE_LAYER, footprint: { width: 1.1, height: 0.7 }, targetSize: { axis: "width", cells: 2 } },
  bush: { file: "bush.png", kind: "foliage", layer: OBSTACLE_LAYER, footprint: { width: 0.7, height: 0.5 }, targetSize: { axis: "width", cells: 1.5 } },
};
