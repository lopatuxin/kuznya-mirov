// Строит рельеф локации по описанию из форм — горы, холмы, площадки, русла — и пишет файл рельефа
// игры, а по разделу `covers` — маски покрытий рядом с ним, в папке `terrain/`. Одно описание всегда
// даёт одни и те же файлы: случайность только от `seed` описания.
// Запуск: `node tools/location/build.mjs art/rpg/village/plan.json`.

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { coverMasks } from "./covers.mjs";
import { terrainText } from "./files.mjs";
import { applyBlocks, applyCracks, applyErosion, applyStrata, applyTalus } from "./mountains.mjs";
import { grayPng, rgbaPng } from "./png.mjs";
import { readPlan } from "./plan.mjs";
import { Grid, applyChannel, applyHill, applyNoise, applyPad, applyRange, applySmooth } from "./terrain.mjs";
import { TINT_PATH, tintMap } from "./tint.mjs";

const APPLY = {
  noise: applyNoise,
  range: applyRange,
  hill: applyHill,
  pad: applyPad,
  channel: applyChannel,
  smooth: applySmooth,
  strata: applyStrata,
  blocks: applyBlocks,
  cracks: applyCracks,
  erosion: applyErosion,
  talus: applyTalus,
};

/** Сетка высот по описанию: операции по порядку, поздняя меняет сделанное ранней. */
export function buildGrid(plan) {
  const { size, density, operations } = readPlan(plan);
  const grid = new Grid(size[0], size[1], density);
  for (const operation of operations) APPLY[operation.op](grid, operation);
  return grid;
}

/** Покрытия по описанию: слои для файла рельефа (нет `covers` — `undefined`) и маски слоёв. */
export function buildCovers(plan, grid) {
  const { covers } = readPlan(plan);
  return { layers: covers, masks: covers ? coverMasks(grid, covers) : [] };
}

function main() {
  const [planPath] = process.argv.slice(2);
  if (!planPath) throw new Error("node tools/location/build.mjs <описание.json>");
  let plan;
  try {
    plan = JSON.parse(readFileSync(planPath, "utf8"));
  } catch (error) {
    throw new Error(`${planPath}: ${error.message}`);
  }
  const grid = buildGrid(plan);
  const { layers, masks } = buildCovers(plan, grid);
  const { tint } = readPlan(plan);
  const target = resolve(dirname(planPath), plan.terrain);
  writeFileSync(target, terrainText(grid, plan.water, layers, tint && TINT_PATH));
  if (tint) {
    const { width, height, pixels } = tintMap(grid, tint);
    const file = resolve(dirname(target), TINT_PATH);
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, rgbaPng(width, height, pixels));
    console.log(`карта цвета ${width} × ${height} → ${file}`);
  }
  for (const { mask, width, height, pixels } of masks) {
    const file = resolve(dirname(target), mask);
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, grayPng(width, height, pixels));
    console.log(`маска ${width} × ${height} → ${file}`);
  }
  let low = Infinity;
  let high = -Infinity;
  for (const h of grid.h) {
    low = Math.min(low, h);
    high = Math.max(high, h);
  }
  console.log(`рельеф ${grid.width} × ${grid.height} → ${target}`);
  console.log(`высоты от ${low.toFixed(2)} до ${high.toFixed(2)}`);
}

// Тест импортирует функции этого файла, и импорт не должен запускать сборку.
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}
