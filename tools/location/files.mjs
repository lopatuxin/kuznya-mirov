// Текст файла рельефа — в том же виде, что пишет редактор (`web/src/editor/terrainFile.ts`).

/** Число в файле рельефа: до сотых без лишних нулей, `-0` — `0`. */
export function hundredths(value) {
  const rounded = Math.round(value * 100) / 100;
  return rounded === 0 ? "0" : String(rounded);
}

/** Файл рельефа: `water` первой строкой, затем `heights` по строке сетки на строку файла. */
export function terrainText(grid, water) {
  const lines = ["{"];
  if (water) lines.push(`  "water": { "level": ${hundredths(water.level)}, "color": ${JSON.stringify(water.color)} },`);
  lines.push('  "heights": [');
  for (let row = 0; row < grid.rows; row++) {
    const numbers = Array.from(grid.h.subarray(row * grid.cols, (row + 1) * grid.cols), hundredths);
    lines.push(`    [${numbers.join(", ")}]${row + 1 < grid.rows ? "," : ""}`);
  }
  lines.push("  ]", "}");
  return lines.join("\n") + "\n";
}
