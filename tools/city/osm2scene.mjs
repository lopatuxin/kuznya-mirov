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
import { random } from "./geo.mjs";

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

export async function run(configPath, root = process.cwd()) {
  const config = JSON.parse(await readFile(configPath, "utf8"));
  const osm = JSON.parse(await readFile(resolve(root, config.osm), "utf8"));
  const city = buildCity(osm, config);
  const out = resolve(root, config.out);
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
  await writeFile(join(out, "scene.json"), sceneText([...city.objects, ...kept], rest));
  await writeFile(
    join(out, "terrain.json"),
    terrainText({ water: { level: config.water.level, color: config.water.color }, tint: "terrain/tint.png", heights: city.heights }),
  );
  await writeFile(join(out, "tables", "addresses.json"), `[\n${city.addresses.map((a) => "  " + JSON.stringify(a)).join(",\n")}\n]\n`);
  const tint = tintImage(city, config);
  await writeFile(join(out, "terrain", "tint.png"), tint.png);
  return { size: [city.width, city.height], kept: kept.length, ...city.stats };
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

