// Сборка деревни ролевой игры из уже нарисованных кусков («Фаза-02-5-первая-нарисованная-локация»,
// план реализации, шаги 5–6): режет широкие стены на полоски по столбцам (требование 13), кладёт
// куски каталога (`rpg-village/catalog.mjs`) по расположению (`rpg-village/layout.mjs`) в
// `games/rpg/scene.json`, дописывает их картинки и сглаживание (`smooth: true`, требование фазы
// «Картинки») в `game.json`, дописывает авторство кусков в CREDITS.txt. Запускать после
// `node scripts/buildRpgArt.mjs` (пишет CREDITS.txt персонажей и часть `files.images`, которую этот
// скрипт не трогает) и после `node ../tools/art/buildVillagePieces.mjs` (кладёт обработанные куски в
// `games/rpg/images/`). Запуск: `node scripts/buildRpgVillageScene.mjs` из `web/`. Детерминирован.

import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { PNG } from "pngjs";
import { KNOWN_KINDS, MARKER_LAYER, OBSTACLE_LAYER, PIECES } from "./rpg-village/catalog.mjs";
import { ENEMIES, HERO_START, PLACEMENTS, SCENE_HEIGHT, SCENE_WIDTH } from "./rpg-village/layout.mjs";
import { buildBottomAlignedStrip, columnBottoms, mergeCellsIntoRects } from "./rpg-village/wallStrip.mjs";

const scriptDir = fileURLToPath(new URL(".", import.meta.url));
const gameDir = resolve(scriptDir, "../../games/rpg");
const imagesDir = resolve(gameDir, "images");
// Требование 7: необрезанные стены (`file` каталога у `wallStrip`) — временный кусок, его пишет
// `tools/art/buildVillagePieces.mjs` вне `games/`, сюда эта сборка их и читает (сам `output` кладёт
// в `imagesDir`, как обычно — им пользуется игра).
const rawImagesDir = resolve(scriptDir, "../../tools/art/.build");

const TILE = 96; // требование 1 фазы: масштаб деревни — 96 точек на клетку
const MAX_OBSTACLES = 200; // нефункциональное требование фазы: поиск пути не должен тормозить

// Требование 21 прежней сборки сцены (buildRpgScene.mjs, перенесено сюда — оно не зависит от
// текстовой карты): рамка врага — от ступней до макушки его фигуры в кадре стойки лицом вниз,
// измерено на кадре 18 (строка «вниз», столбец 0) основного листа: гоблин — макушка на 14-й точке
// из 64, орк — на 17-й, у обоих ступни на 61-й.
const STANDING_DOWN_FRAME = 18;
const ENEMY_BODY_WIDTH = 0.9;
const ENEMY_BODY_HEIGHT = {
  goblin: (61 - 14 + 1) / 32,
  orc: (61 - 17 + 1) / 32,
};

function readPngFile(path) {
  return PNG.sync.read(readFileSync(path));
}

function writePngFile(path, png) {
  writeFileSync(path, PNG.sync.write(png));
}

// ---------------------------------------------------------------------------------------------
// Метаданные уже обработанных кусков: у стен — результат резки на полоски (перезаписывает файл
// куска полосками, требование 13), у остального — просто размер готового файла в точках.
// ---------------------------------------------------------------------------------------------

// Требование 6 («крайний случай» плана): неизвестный вид или слой куска останавливает сборку с
// сообщением, каким именно куском — а не падает TypeError чуть дальше на `undefined`.
export function validatePiece(name, piece) {
  if (!KNOWN_KINDS.has(piece.kind)) throw new Error(`кусок «${name}»: неизвестный вид «${piece.kind}»`);
  if (typeof piece.layer !== "number") throw new Error(`кусок «${name}»: не задан слой`);
}

function loadPieceMeta(name) {
  const piece = PIECES[name];
  if (!piece) throw new Error(`расположение ссылается на кусок «${name}», которого нет в каталоге`);
  validatePiece(name, piece);
  if (piece.kind !== "wallStrip") {
    const png = readPngFile(resolve(imagesDir, piece.file));
    return { kind: piece.kind, width: png.width, height: png.height, piece };
  }

  const png = readPngFile(resolve(rawImagesDir, piece.file));
  const bottoms = columnBottoms(png.data, png.width, png.height, TILE);
  const strip = buildBottomAlignedStrip(png.data, png.width, png.height, TILE, bottoms);
  const out = new PNG({ width: strip.width, height: strip.height });
  strip.data.copy(out.data);
  writePngFile(resolve(imagesDir, piece.output), out);
  return { kind: "wallStrip", width: strip.width, height: strip.height, bottoms, frameCount: strip.frameCount, piece };
}

function loadAllPieceMeta() {
  const names = new Set(PLACEMENTS.map((p) => p.piece));
  const meta = new Map();
  for (const name of names) meta.set(name, loadPieceMeta(name));
  return meta;
}

// ---------------------------------------------------------------------------------------------
// Объявления картинок — требование фазы «Картинки»: у всех картинок деревни и травы `smooth: true`.
// ---------------------------------------------------------------------------------------------

function imageDeclarationFor(name, meta) {
  const path = `images/${meta.piece.output ?? meta.piece.file}`;
  if (meta.kind === "wallStrip") {
    return { path, frames: meta.frameCount, columns: meta.frameCount, frame_by: "frame", size: [1, meta.height / TILE], anchor: "bottom", smooth: true };
  }
  if (meta.kind === "ground") {
    return { path, size: [meta.width / TILE, meta.height / TILE], anchor: "top_left", smooth: true };
  }
  return { path, size: [meta.width / TILE, meta.height / TILE], anchor: "bottom", smooth: true };
}

function buildVillageImages(pieceMeta) {
  const images = {};
  for (const [name, meta] of pieceMeta) images[name] = imageDeclarationFor(name, meta);
  images.grass = { path: "images/grass.png", frames: 16, columns: 4, smooth: true };
  return images;
}

// ---------------------------------------------------------------------------------------------
// Объекты сцены из расположения куска (требования 2, 13, 14): диагональ — два вправо (или, при
// отражении, два влево) на одну клетку вверх; столбец i строится в клетках на расстоянии i от
// точки постановки `(x, y)` по обеим осям — рисуемая полоска по измеренному в картинке низу
// (требование 13), невидимое препятствие — по формуле лесенки (требование 14), независимо друг от
// друга: полоска не обязана попадать точно в клетку лесенки, это лишь грубое приближение для
// столкновений.
// ---------------------------------------------------------------------------------------------

// Требование 1 плана («стену короче целого куска можно поставить частью её полосок»): `placement.
// columns`, если задан, ограничивает и рисуемые полоски, и лесенку препятствий первыми `columns`
// столбцами куска — сам кусок (`meta`) остаётся при этом целым (его read/output не режут заново).
export function buildWallStripObjects(placement, imageName, meta) {
  const { bottoms, height } = meta;
  const heightCells = height / TILE;
  const anchorBottom = bottoms[0];
  const piece = PIECES[imageName];
  const columnCount = placement.columns ?? meta.frameCount;
  const objects = [];
  for (let i = 0; i < columnCount; i++) {
    const worldX = placement.flip ? placement.x - i : placement.x + i;
    if (bottoms[i] < 0) continue; // столбец целиком прозрачен — рисовать нечего (полоска не строится)
    const footWorldY = placement.y + 1 + (bottoms[i] - anchorBottom) / TILE;
    objects.push({
      name: `${placement.name}_strip_${i}`,
      position: [worldX, footWorldY - heightCells],
      size: [1, heightCells],
      layer: piece.layer,
      image: imageName,
      frame: i,
      ...(placement.flip ? { flip_x: true } : {}),
    });
  }
  // Требование 9: лесенка препятствий берётся из каталога (`piece.obstacleCells`), а не считается
  // здесь заново из числа столбцов — сборка лишь переносит её в мировые координаты постановки.
  const obstacleCells = piece.obstacleCells.slice(0, columnCount).map(({ x, y }) => ({
    x: placement.flip ? placement.x - x : placement.x + x,
    y: placement.y + y,
  }));
  for (const [j, rect] of mergeCellsIntoRects(obstacleCells).entries()) {
    objects.push({ name: `${placement.name}_wall_${j}`, position: [rect.x, rect.y], size: [rect.width, rect.height], layer: piece.layer, obstacle: true });
  }
  return objects;
}

function buildGroundObject(placement, imageName, meta) {
  const piece = PIECES[imageName];
  return [
    {
      name: placement.name,
      position: [placement.x, placement.y],
      size: [meta.width / TILE, meta.height / TILE],
      layer: piece.layer,
      image: imageName,
      ...(placement.flip ? { flip_x: true } : {}),
    },
  ];
}

// Требование 15 (постройки и столб — тем же простым видом, «первого уровня»): один объект — малый
// прямоугольник препятствия у основания (`footprint` каталога), картинка крупнее с `anchor:
// "bottom"` рисуется из него вверх и по центру (движок сам центрирует по точке привязки).
function buildFootprintObject(placement, imageName) {
  const { footprint, layer } = PIECES[placement.piece];
  return [
    {
      name: placement.name,
      position: [placement.x, placement.y],
      size: [footprint.width, footprint.height],
      layer,
      obstacle: true,
      image: imageName,
    },
  ];
}

function buildPlacementObjects(placement, index, pieceMeta) {
  const meta = pieceMeta.get(placement.piece);
  const named = { ...placement, name: `${placement.piece}_${index}` };
  if (meta.kind === "wallStrip") return buildWallStripObjects(named, placement.piece, meta);
  if (meta.kind === "ground") return buildGroundObject(named, placement.piece, meta);
  return buildFootprintObject(named, placement.piece);
}

// ---------------------------------------------------------------------------------------------
// Герой, враги и отметка щелчка — без изменений в свойствах от прежней сборки (buildRpgScene.mjs,
// «Фаза-02-враг-на-пути», требования 17, 21), но по месту из `layout.mjs`, а не из текстовой карты.
// ---------------------------------------------------------------------------------------------

function buildHeroObject() {
  return {
    name: "hero",
    position: [HERO_START.x + (1 - 0.75) / 2, HERO_START.y + (1 - 0.5) / 2],
    size: [0.75, 0.5],
    layer: OBSTACLE_LAYER,
    hero: true,
    camera_follows: true,
    walk_speed: 4,
    image: "hero",
    frame: STANDING_DOWN_FRAME,
    health: 100,
    max_health: 100,
    damage: 10,
    keys: { MouseLeft: { press: [["walk_to", "cursor"]] } },
  };
}

function buildEnemyObjects() {
  const counters = {};
  return ENEMIES.map(({ enemy, x, y }) => {
    counters[enemy] = (counters[enemy] ?? 0) + 1;
    const bodyHeight = ENEMY_BODY_HEIGHT[enemy];
    return {
      name: `${enemy}_${counters[enemy]}`,
      position: [x - ENEMY_BODY_WIDTH / 2, y - bodyHeight / 2],
      size: [ENEMY_BODY_WIDTH, bodyHeight],
      layer: OBSTACLE_LAYER,
      enemy_unit: true,
      obstacle: true,
      enemy,
      image: enemy,
      frame: STANDING_DOWN_FRAME,
      keys: { MouseLeft: { press: [["target", false]] } },
      on_click: [["target", true]],
    };
  });
}

function buildMarkerObject() {
  return {
    name: "marker",
    position: [0, 0],
    size: [0.8, 0.8],
    layer: MARKER_LAYER,
    marker: true,
    opacity: 0,
    image: "marker",
    keys: { MouseLeft: { press: [["position", "cursor"], ["opacity", 1]] } },
  };
}

// Требование 12: под водой — невидимые препятствия, под мостом их нет. `waterCells` каталога —
// лесенка относительно нижнего (по картинке) угла куска ручья, тем же способом, что и у стены
// (`piece.obstacleCells`, `buildWallStripObjects`) — сборка лишь переносит её в мировые координаты
// конкретной постановки и сливает клетки в прямоугольники.
function buildWaterObjects(pieceMeta) {
  const objects = [];
  let counter = 0;
  for (const placement of PLACEMENTS) {
    const piece = PIECES[placement.piece];
    if (!piece.waterCells) continue;
    const { width, height } = pieceMeta.get(placement.piece);
    const lowRow = placement.y + height / TILE - 1;
    // Отражённый кусок: столбец `x` исходной картинки встаёт с правого края её рамки.
    // Клетки целиком вне сцены отбрасываются: движок не грузит объект, не задевающий сцену.
    const cells = piece.waterCells
      .map(({ x, y }) => ({ x: placement.flip ? placement.x + width / TILE - 1 - x : placement.x + x, y: lowRow + y }))
      .filter(({ x, y }) => x + 1 > 0 && x < SCENE_WIDTH && y + 1 > 0 && y < SCENE_HEIGHT);
    for (const rect of mergeCellsIntoRects(cells)) {
      objects.push({ name: `water_${counter++}`, position: [rect.x, rect.y], size: [rect.width, rect.height], layer: OBSTACLE_LAYER, obstacle: true });
    }
  }
  return objects;
}

// Требование 11: трава — вся сцена через `ground`, требование 6 фазы: плитка берётся по месту
// клетки в наборе 4×4 — `tools/art/tile.mjs` делает саму плитку бесшовной при повторе именно так.
export function buildGrassGround() {
  const cells = [];
  for (let y = 0; y < SCENE_HEIGHT; y++) {
    const row = [];
    for (let x = 0; x < SCENE_WIDTH; x++) row.push((y % 4) * 4 + (x % 4));
    cells.push(row);
  }
  return [{ image: "grass", cells }];
}

export function buildScene(pieceMeta) {
  const objects = [
    ...PLACEMENTS.flatMap((placement, index) => buildPlacementObjects(placement, index, pieceMeta)),
    ...buildWaterObjects(pieceMeta),
    ...buildEnemyObjects(),
    buildHeroObject(),
    buildMarkerObject(),
  ];
  return { objects, ground: buildGrassGround() };
}

// ---------------------------------------------------------------------------------------------
// CREDITS.txt: дописывает авторство нарисованных кусков деревни к части персонажей, которую пишет
// `buildRpgArt.mjs` (запускается первым — см. комментарий в начале файла).
// ---------------------------------------------------------------------------------------------

function villageCreditsText() {
  return [
    "Деревня — трава, дорожки, ручей и мост, стены, постройки, деревья, валуны, кусты",
    "  Источник: изображения, сгенерированные нейросетью GPT Image 2.5 Sunburst по запросам автора проекта, 2026-09-28",
    "  Обработка: tools/art/ — снят однотонный фон, обрезано по содержимому, приведено к масштабу игры",
    "",
  ].join("\n");
}

// Повторный запуск не должен дублировать блок деревни (пайплайн детерминирован — та же запись, а не
// накопление копий): существующий блок деревни, если он уже был дописан раньше, сначала срезается.
export function mergeVillageCredits(existingCredits) {
  const villageHeading = villageCreditsText().split("\n")[0];
  const headingIndex = existingCredits.indexOf(villageHeading);
  const characterCredits = existingCredits.slice(0, headingIndex === -1 ? undefined : headingIndex).replace(/\n*$/, "\n\n");
  return characterCredits + villageCreditsText();
}

// ---------------------------------------------------------------------------------------------
// Точка входа: режет стены на полоски, собирает scene.json, дописывает game.json и CREDITS.txt.
// ---------------------------------------------------------------------------------------------

const isMainModule = import.meta.url === pathToFileURL(process.argv[1] ?? "").href;
if (isMainModule) {
  const pieceMeta = loadAllPieceMeta();
  const scene = buildScene(pieceMeta);
  writeFileSync(resolve(gameDir, "scene.json"), JSON.stringify(scene, null, 2) + "\n");

  const obstacleCount = scene.objects.filter((o) => o.obstacle).length;
  console.log(`scene.json: ${scene.objects.length} объектов, из них ${obstacleCount} препятствий (предел ${MAX_OBSTACLES})`);
  if (obstacleCount > MAX_OBSTACLES) throw new Error(`препятствий ${obstacleCount} больше предела ${MAX_OBSTACLES} — поиск пути будет тормозить`);

  const gameJsonPath = resolve(gameDir, "game.json");
  const gameJson = JSON.parse(readFileSync(gameJsonPath, "utf8"));
  gameJson.scene = { width: SCENE_WIDTH, height: SCENE_HEIGHT, background: "#4f8a3c", view_height: 12, y_sort: true };
  // Ревью: старые ключи местности LPC (terrain/tree/rock/wall_<N>x<N>) game.json не переживают смену
  // подхода сами — их явно убираем перед тем, как добавить объявления деревни (требование 17: LPC
  // местность из игры уходит).
  const survivingImages = Object.fromEntries(Object.entries(gameJson.files.images ?? {}).filter(([name]) => !["terrain", "tree", "rock"].includes(name) && !/^wall_\d+x\d+$/.test(name)));
  gameJson.files.images = { ...survivingImages, ...buildVillageImages(pieceMeta) };
  writeFileSync(gameJsonPath, JSON.stringify(gameJson, null, 2) + "\n");
  console.log("game.json обновлён (scene, files.images: деревня)");

  const creditsPath = resolve(gameDir, "CREDITS.txt");
  writeFileSync(creditsPath, mergeVillageCredits(readFileSync(creditsPath, "utf8")));
  console.log("CREDITS.txt дополнен авторством деревни");
}
