// Текст файла рельефа — в том же виде, что пишет редактор (`web/src/editor/terrainFile.ts`).

/** Число в файле рельефа: до сотых без лишних нулей, `-0` — `0`. */
export function hundredths(value) {
  const rounded = Math.round(value * 100) / 100;
  return rounded === 0 ? "0" : String(rounded);
}

/**
 * Файл рельефа: `water` первой строкой, затем `covers` по слою на строку, затем `heights` по строке
 * сетки на строку файла. Слой покрытий — `{ material, mask? }`.
 */
export function terrainText(grid, water, covers) {
  const lines = ["{"];
  if (water) lines.push(`  "water": { "level": ${hundredths(water.level)}, "color": ${JSON.stringify(water.color)} },`);
  if (covers) {
    lines.push('  "covers": [');
    covers.forEach(({ material, mask }, index) => {
      const fields = [`"material": ${JSON.stringify(material)}`];
      if (mask !== undefined) fields.push(`"mask": ${JSON.stringify(mask)}`);
      lines.push(`    { ${fields.join(", ")} }${index + 1 < covers.length ? "," : ""}`);
    });
    lines.push("  ],");
  }
  lines.push('  "heights": [');
  for (let row = 0; row < grid.rows; row++) {
    const numbers = Array.from(grid.h.subarray(row * grid.cols, (row + 1) * grid.cols), hundredths);
    lines.push(`    [${numbers.join(", ")}]${row + 1 < grid.rows ? "," : ""}`);
  }
  lines.push("  ]", "}");
  return lines.join("\n") + "\n";
}
