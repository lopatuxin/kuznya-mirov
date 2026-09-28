// Сборка `games/rpg/scene.json` из текстовой карты локации («Фаза-02-враг-на-пути», пункты
// 9–15, 17, 21): три слоя земли (трава, тропинка/вода, мелочи) по номерам плиток из
// `rpgTerrainTiles.mjs`, препятствия — стены и вода, слитые в прямоугольники, отдельные
// деревья/камни, враги (каждый — прямоугольник по центру своего проёма) и герой, отметка щелчка.
// Запуск: `node scripts/buildRpgScene.mjs` из `web/`. Детерминирован, сети не требует.

import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseLocationMap, extractRectangles, CELL } from "./rpgLocationMap.mjs";
import {
  neighborBitmask,
  pathTileIndex,
  waterTileIndex,
  grassVariantTile,
  decorationTile,
  hasDecoration,
} from "./rpgTerrainTiles.mjs";
import { TREE_FOOTPRINT_SIZE, ROCK_FOOTPRINT_SIZE, wallImageName } from "./buildRpgArt.mjs";

const scriptDir = fileURLToPath(new URL(".", import.meta.url));
const lpcDir = resolve(scriptDir, "rpg-lpc");
const gameDir = resolve(scriptDir, "../../games/rpg");

const OBSTACLE_LAYER = 2;

// Стойка лицом вниз, формула требования 6: 9 × строка + столбец, строка 2 — ходьба вниз, столбец
// 0 — стойка.
const STANDING_DOWN_FRAME = 18;

// Клетки, которые считаются «тем же типом» для автотайла тропинки — герой и враги стоят там же,
// где иначе была бы обычная клетка коридора.
const PATH_LIKE = new Set([CELL.PATH, CELL.HERO, CELL.GOBLIN_1, CELL.GOBLIN_2, CELL.GOBLIN_3, CELL.ORC]);

export function buildGroundLayers(map) {
  const { width, height, rows } = map;
  const isPath = (x, y) => PATH_LIKE.has(rows[y][x]);
  const isWater = (x, y) => rows[y][x] === CELL.WATER;
  const isGlade = (x, y) => rows[y][x] === CELL.GLADE;
  const isWall = (x, y) => rows[y][x] === CELL.WALL;
  // Ревью: кайма (переход к траве) рисуется со стороны соседа, который сама плитка не считает
  // «своим»; стена — не трава, но и не то, у чего должна быть каёмка — тропинка и вода подходят
  // к её подножию вплотную. Стену считаем «своей» только для расчёта каймы (бит соседа), не для
  // самой принадлежности клетки к тропинке/воде.
  const isPathOrWall = (x, y) => isPath(x, y) || isWall(x, y);
  const isWaterOrWall = (x, y) => isWater(x, y) || isWall(x, y);

  const grassCells = [];
  const featureCells = [];
  const decorationCells = [];
  for (let y = 0; y < height; y++) {
    const grassRow = [];
    const featureRow = [];
    const decorationRow = [];
    for (let x = 0; x < width; x++) {
      grassRow.push(grassVariantTile(x, y));
      if (isPath(x, y)) {
        featureRow.push(pathTileIndex(neighborBitmask(width, height, x, y, isPathOrWall)));
      } else if (isWater(x, y)) {
        featureRow.push(waterTileIndex(neighborBitmask(width, height, x, y, isWaterOrWall)));
      } else {
        featureRow.push(-1);
      }
      decorationRow.push(isGlade(x, y) && hasDecoration(x, y) ? decorationTile(x, y) : -1);
    }
    grassCells.push(grassRow);
    featureCells.push(featureRow);
    decorationCells.push(decorationRow);
  }

  return [
    { image: "terrain", cells: grassCells },
    { image: "terrain", cells: featureCells },
    { image: "terrain", cells: decorationCells },
  ];
}

function findCells(map, matchChar) {
  const { width, height, rows } = map;
  const found = [];
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (rows[y][x] === matchChar) found.push({ x, y });
    }
  }
  return found;
}

const ENEMY_CATALOG = {
  [CELL.GOBLIN_1]: "goblin",
  [CELL.GOBLIN_2]: "goblin",
  [CELL.GOBLIN_3]: "goblin",
  [CELL.ORC]: "orc",
};

// Требование 9 (ревью): рамка врага — от ступней до макушки его фигуры в кадре стойки лицом вниз,
// не одна общая высота для всех, иначе полоска здоровья (которая встаёт над верхним краем рамки)
// у одного персонажа висит дальше от головы, чем у другого. Измерено на кадре 18 (строка «вниз»,
// столбец 0) основного листа: гоблин — макушка на 14-й точке из 64, орк — на 17-й, у обоих ступни
// на 61-й (тот же зазор 2 точки от низа кадра, что и `CHARACTER_IMAGE_OFFSET`); высота — разница,
// переведённая в клетки (32 точки на клетку).
const ENEMY_BODY_WIDTH = 0.9;
const ENEMY_BODY_HEIGHT = {
  goblin: (61 - 14 + 1) / 32,
  orc: (61 - 17 + 1) / 32,
};

export function buildObjects(map) {
  const objects = [];

  // Требование 13: соседние клетки стены сливаются в прямоугольники, не только строки/столбцы.
  const wallRects = extractRectangles(map, CELL.WALL);
  wallRects.forEach((rect, index) => {
    objects.push({
      name: `wall_${index}`,
      position: [rect.x, rect.y],
      size: [rect.width, rect.height],
      layer: OBSTACLE_LAYER,
      obstacle: true,
      image: wallImageName(rect.width, rect.height),
    });
  });

  // Требование 14: дерево — footprint 0,8 × 0,5 у основания, по центру клетки и у её нижнего края.
  findCells(map, CELL.TREE).forEach(({ x, y }, index) => {
    const [fw, fh] = TREE_FOOTPRINT_SIZE;
    objects.push({
      name: `tree_${index}`,
      position: [x + (1 - fw) / 2, y + 1 - fh],
      size: [fw, fh],
      layer: OBSTACLE_LAYER,
      obstacle: true,
      image: "tree",
    });
  });

  findCells(map, CELL.ROCK).forEach(({ x, y }, index) => {
    const [fw, fh] = ROCK_FOOTPRINT_SIZE;
    objects.push({
      name: `rock_${index}`,
      position: [x + (1 - fw) / 2, y + 1 - fh],
      size: [fw, fh],
      layer: OBSTACLE_LAYER,
      obstacle: true,
      image: "rock",
    });
  });

  // Требование 13 (ревью): вода — одно препятствие на каждый связный прямоугольный кусок, а не
  // общий охватывающий прямоугольник на весь пруд.
  const waterRects = extractRectangles(map, CELL.WATER);
  waterRects.forEach((rect, index) => {
    objects.push({
      name: `water_${index}`,
      position: [rect.x, rect.y],
      size: [rect.width, rect.height],
      layer: OBSTACLE_LAYER,
      obstacle: true,
    });
  });

  // Требование 21 (ревью, пункт 7): враг — enemy_unit и obstacle, тело шириной 0,9 (высота — от
  // ступней до макушки, своя у каждого вида, см. `ENEMY_BODY_HEIGHT`, ревью пункт 9) по центру его
  // проёма (карта отмечает обе клетки проёма одним символом — центрируем по их общему
  // прямоугольнику, а не сдвигом от одной клетки, чтобы тело не задевало стену проёма шириной 2).
  const enemyCounters = {};
  for (const [cellChar, catalogKey] of Object.entries(ENEMY_CATALOG)) {
    const cells = findCells(map, cellChar);
    if (cells.length === 0) continue;
    const minX = Math.min(...cells.map((c) => c.x));
    const maxX = Math.max(...cells.map((c) => c.x));
    const minY = Math.min(...cells.map((c) => c.y));
    const maxY = Math.max(...cells.map((c) => c.y));
    const boxWidth = maxX - minX + 1;
    const boxHeight = maxY - minY + 1;
    const bodyHeight = ENEMY_BODY_HEIGHT[catalogKey];
    enemyCounters[catalogKey] = (enemyCounters[catalogKey] ?? 0) + 1;
    objects.push({
      name: `${catalogKey}_${enemyCounters[catalogKey]}`,
      position: [minX + (boxWidth - ENEMY_BODY_WIDTH) / 2, minY + (boxHeight - bodyHeight) / 2],
      size: [ENEMY_BODY_WIDTH, bodyHeight],
      layer: OBSTACLE_LAYER,
      enemy_unit: true,
      obstacle: true,
      enemy: catalogKey,
      image: catalogKey,
      frame: STANDING_DOWN_FRAME,
      keys: { MouseLeft: { press: [["target", false]] } },
      on_click: [["target", true]],
    });
  }

  // Требование 17: герой — footprint 0,75 × 0,5, по центру клетки входа, кадр стойки лицом вниз.
  const heroCell = findCells(map, CELL.HERO)[0];
  objects.push({
    name: "hero",
    position: [heroCell.x + (1 - 0.75) / 2, heroCell.y + (1 - 0.5) / 2],
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
  });

  // Отметка щелчка — без изменений от фазы 01 («Кольцо отметки остаётся из генератора»).
  objects.push({
    name: "marker",
    position: [0, 0],
    size: [0.8, 0.8],
    layer: 1,
    marker: true,
    opacity: 0,
    image: "marker",
    keys: { MouseLeft: { press: [["position", "cursor"], ["opacity", 1]] } },
  });

  return objects;
}

export function buildScene(locationText) {
  const map = parseLocationMap(locationText);
  return {
    objects: buildObjects(map),
    ground: buildGroundLayers(map),
  };
}

const isMainModule = import.meta.url === pathToFileURL(process.argv[1] ?? "").href;
if (isMainModule) {
  const locationText = readFileSync(resolve(lpcDir, "location.txt"), "utf8");
  const scene = buildScene(locationText);
  writeFileSync(resolve(gameDir, "scene.json"), JSON.stringify(scene, null, 2) + "\n");
  console.log(`scene.json: ${scene.objects.length} объектов, ${scene.ground.length} слоя земли`);
}
