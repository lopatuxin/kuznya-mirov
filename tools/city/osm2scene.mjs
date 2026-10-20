// Переводит выгрузку OpenStreetMap в карту игры Кузни: объекты карты в `scene.json` (с признаком `map`),
// рельеф с водой `terrain.json` и карту цвета земли `terrain/tint.png`. Объекты сцены без `map` — то, что
// автор поставил сам, — остаются на месте; объекты карты пишутся заново.
// Запуск: node tools/city/osm2scene.mjs art/transport/city.json

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { buildCity } from "./city.mjs";
import { encodePng } from "./png.mjs";
import { createRaster, fillPolygon } from "./raster.mjs";
import { random, round } from "./geo.mjs";

const TINT_WIDTH = 2048;

/** Карта цвета земли: виды земли — цветами из настроек, деленными на фон сцены; неровность — мягким шумом. */
export function tintImage(city, config) {
  const per = TINT_WIDTH / city.width;
  const raster = createRaster(city.width, city.height, per);
  const kinds = Object.keys(config.ground.colors);
  for (const kind of config.ground.order) {
    const index = kinds.indexOf(kind) + 1;
    if (index === 0) throw new Error(`вид земли «${kind}» без цвета`);
    for (const z of city.zones) if (z.kind === kind) fillPolygon(raster, [z.ring], index);
  }
  const bg = hex(config.background);
  const colors = [hex(config.ground.default), ...kinds.map((k) => hex(config.ground.colors[k]))];
  const shade = new Set(config.ground.shaded || []);
  const noise = valueNoise(raster.width, raster.height, 48, config.seed ?? 1);
  const rgba = new Uint8Array(raster.width * raster.height * 4);
  for (let i = 0; i < raster.width * raster.height; i++) {
    const kindIndex = raster.data[i];
    const c = colors[kindIndex];
    const n = 1 + (noise[i] - 0.5) * config.ground.noise;
    for (let ch = 0; ch < 3; ch++) rgba[i * 4 + ch] = Math.max(0, Math.min(255, Math.round((128 * c[ch] * n) / bg[ch])));
    rgba[i * 4 + 3] = kindIndex > 0 && shade.has(kinds[kindIndex - 1]) ? 215 : 255;
  }
  return { width: raster.width, height: raster.height, png: encodePng(raster.width, raster.height, rgba) };
}

function hex(color) {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(color);
  if (!m) throw new Error(`цвет «${color}» не #rrggbb`);
  return [1, 2, 3].map((i) => Number.parseInt(m[i], 16));
}

/** Плавный шум от 0 до 1 по сетке шага `step` точек: билинейно между случайными узлами. */
function valueNoise(w, h, step, seed) {
  const rand = random(seed * 7919);
  const gw = Math.ceil(w / step) + 2, gh = Math.ceil(h / step) + 2;
  const grid = Array.from({ length: gw * gh }, rand);
  const out = new Float32Array(w * h);
  for (let y = 0; y < h; y++) {
    const gy = y / step, y0 = Math.floor(gy), fy = gy - y0;
    for (let x = 0; x < w; x++) {
      const gx = x / step, x0 = Math.floor(gx), fx = gx - x0;
      const a = grid[y0 * gw + x0], b = grid[y0 * gw + x0 + 1], c = grid[(y0 + 1) * gw + x0], d = grid[(y0 + 1) * gw + x0 + 1];
      const sx = fx * fx * (3 - 2 * fx), sy = fy * fy * (3 - 2 * fy);
      out[y * w + x] = (a + (b - a) * sx) * (1 - sy) + (c + (d - c) * sx) * sy;
    }
  }
  return out;
}

/** Текст `scene.json`: объект на строку, чтобы файл читался и правился глазами. */
export function sceneText(objects, rest) {
  const lines = objects.map((o) => "    " + JSON.stringify(o));
  const head = Object.entries(rest).map(([k, v]) => `  ${JSON.stringify(k)}: ${JSON.stringify(v)},\n`).join("");
  return `{\n${head}  "objects": [\n${lines.join(",\n")}\n  ]\n}\n`;
}

/** Текст `terrain.json`: строка высот на строку файла. */
export function terrainText(terrain) {
  const { heights, ...rest } = terrain;
  const head = Object.entries(rest).map(([k, v]) => `  ${JSON.stringify(k)}: ${JSON.stringify(v)},\n`).join("");
  return `{\n${head}  "heights": [\n${heights.map((r) => "    " + JSON.stringify(r)).join(",\n")}\n  ]\n}\n`;
}

/** Текст `game.json` с отступом в два пробела, но плоский объект в списке — одной строкой, как рукава перекрёстков. */
export function gameText(value, indent = "") {
  const inner = indent + "  ";
  if (Array.isArray(value)) {
    if (value.length === 0) return "[]";
    const item = (v) => (isFlat(v) ? `{ ${Object.entries(v).map(([k, x]) => `${JSON.stringify(k)}: ${JSON.stringify(x)}`).join(", ")} }` : gameText(v, inner));
    return `[\n${value.map((v) => inner + item(v)).join(",\n")}\n${indent}]`;
  }
  if (isPlain(value)) {
    const entries = Object.entries(value);
    if (entries.length === 0) return "{}";
    return `{\n${entries.map(([k, v]) => `${inner}${JSON.stringify(k)}: ${gameText(v, inner)}`).join(",\n")}\n${indent}}`;
  }
  return JSON.stringify(value);
}

const isPlain = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
const isFlat = (v) => isPlain(v) && Object.keys(v).length > 0 && Object.values(v).every((x) => x === null || typeof x !== "object");

export async function run(configPath, root = process.cwd()) {
  const config = JSON.parse(await readFile(configPath, "utf8"));
  const osm = JSON.parse(await readFile(resolve(root, config.osm), "utf8"));
  const city = buildCity(osm, config);
  return writeCity(city, config, config.name, root);
}

/**
 * Пишет карту `city` в папку игры `config.out`: объекты карты в `scene.json` поверх объектов автора, рельеф,
 * адреса и карту цвета земли. Под карту подстраивается и остальное: машина автора встаёт к конторе (место
 * `office`), камера игрока — над ней, название города `name` — в свойство `town` объекта игры, в `game.json` —
 * виды дорог настроек `road_kinds` и ровно те модели, что названы в сцене, видах дорог, правилах, коде или
 * таблицах: модели прежней карты уходят, новой — добавляются.
 */
export async function writeCity(city, config, name, root = process.cwd()) {
  const out = resolve(root, config.out);
  const office = city.objects.find((o) => o.place === "office");
  if (!office) throw new Error("на карте нет конторы — места office");
  await mkdir(join(out, "terrain"), { recursive: true });
  await mkdir(join(out, "tables"), { recursive: true });

  let kept = [];
  let rest = {};
  try {
    const old = JSON.parse(await readFile(join(out, "scene.json"), "utf8"));
    kept = (old.objects || []).filter((o) => !o.map);
    const { objects: _ignored, ...others } = old;
    rest = others;
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  kept = kept.map((o) => {
    if (o.car) return { ...o, position: [round(office.stop_x - o.size[0] / 2), round(office.stop_y - o.size[1] / 2)] };
    if (o.game) return { ...o, town: name };
    return o;
  });
  const objects = [...city.objects, ...kept];
  await writeFile(join(out, "scene.json"), sceneText(objects, rest));
  await writeGameJson(out, objects, office, config.road_kinds);
  await writeFile(
    join(out, "terrain.json"),
    terrainText({ water: { level: config.water.level, color: config.water.color }, tint: "terrain/tint.png", heights: city.heights }),
  );
  await writeFile(join(out, "tables", "addresses.json"), `[\n${city.addresses.map((a) => "  " + JSON.stringify(a)).join(",\n")}\n]\n`);
  const tint = tintImage(city, config);
  await writeFile(join(out, "terrain", "tint.png"), tint.png);
  return { size: [city.width, city.height], kept: kept.length, ...city.stats };
}

/** Камера игрока — над конторой; виды дорог `roadKinds`, без них — никаких; модели — те, что названы в сцене,
 *  видах дорог, перекрёстках `scene.road_junctions`, правилах, коде или таблицах, по алфавиту. */
async function writeGameJson(out, objects, office, roadKinds) {
  const path = join(out, "game.json");
  const text = await readFile(path, "utf8");
  const game = JSON.parse(text);
  game.scene.player_camera.start = [round(office.stop_x, 1), round(office.stop_y, 1)];
  const files = [game.files.rules, game.files.code, ...Object.values(game.files.tables || {})].filter(Boolean);
  const named = (await Promise.all(files.map((f) => readFile(join(out, f), "utf8")))).join("\n");
  if (roadKinds) game.scene.road_kinds = roadKinds;
  else delete game.scene.road_kinds;
  const inScene = new Set(objects.map((o) => o.model).filter(Boolean));
  const ofRoads = Object.values(roadKinds ?? {}).flatMap((k) => k.piece);
  const junctions = Object.keys(game.scene.road_junctions ?? {});
  const used = new Set([...inScene, ...ofRoads, ...junctions, ...Object.keys(game.files.models).filter((m) => named.includes(`"${m}"`))]);
  game.files.models = Object.fromEntries(
    [...used].sort().map((m) => [m, game.files.models[m] ?? { path: `models/${m}.glb` }]),
  );
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  await writeFile(path, (gameText(game) + "\n").replace(/\n/g, eol));
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const configPath = process.argv[2];
  if (!configPath) {
    console.error("укажи файл настроек города: node tools/city/osm2scene.mjs art/transport/city.json");
    process.exit(2);
  }
  run(resolve(configPath)).then(
    (stats) => console.log(JSON.stringify(stats)),
    (error) => {
      console.error(error.message);
      process.exit(1);
    },
  );
}

