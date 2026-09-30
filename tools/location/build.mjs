// Строит рельеф локации по описанию из форм — горы, холмы, площадки, русла — и пишет файл рельефа
// игры. Одно описание всегда даёт один и тот же файл: случайность только от `seed` описания.
// Запуск: `node tools/location/build.mjs art/rpg/village/plan.json`.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { terrainText } from "./files.mjs";
import { readPlan } from "./plan.mjs";
import { Grid, applyChannel, applyHill, applyNoise, applyPad, applyRange, applySmooth } from "./terrain.mjs";

const APPLY = {
  noise: applyNoise,
  range: applyRange,
  hill: applyHill,
  pad: applyPad,
  channel: applyChannel,
  smooth: applySmooth,
};

/** Сетка высот по описанию: операции по порядку, поздняя меняет сделанное ранней. */
export function buildGrid(plan) {
  const { size, operations } = readPlan(plan);
  const grid = new Grid(size[0], size[1]);
  for (const operation of operations) APPLY[operation.op](grid, operation);
  return grid;
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
  const target = resolve(dirname(planPath), plan.terrain);
  writeFileSync(target, terrainText(grid, plan.water));
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
