// Делает из квадратной картинки нейросети бесшовную плитку земли для игры: края картинки
// смешиваются с её же серединой, и плитка стыкуется сама с собой без шва. Выход — квадрат из
// `--cells` × `--cells` клеток по `--cell-px` точек; в `game.json` он объявляется набором плиток,
// а в `cells` слоя земли каждой клетке сцены пишется плитка по её месту: `(y % N) * N + x % N`.
// Зависимости ставятся один раз: `npm install` в `tools/art/`.

import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import sharp from "sharp";
import { smoothstep } from "./sheet.mjs";

const USAGE = "node tools/art/tile.mjs <вход.png> <выход.png> [--cells N] [--cell-px P]";

function positiveInt(name, text) {
  const value = Number(text);
  if (!Number.isInteger(value) || value < 1) throw new Error(`--${name} должно быть целым числом от 1, а не «${text}»`);
  return value;
}

/** Вес самой картинки в столбце или строке `i` из `size`: 0 у краёв, 1 дальше четверти от края. */
export function edgeWeight(i, size) {
  return smoothstep(0, size / 4, Math.min(i, size - 1 - i));
}

/**
 * Смешивает картинку RGBA с её копией, сдвинутой на половину по оси `axis` (0 — по ширине, 1 — по
 * высоте). У краёв остаётся копия: её крайние точки — соседние точки середины картинки, поэтому
 * плитка стыкуется сама с собой без шва. В середине остаётся картинка, там, где шов у копии.
 */
export function blendWithHalfShift(rgba, width, height, axis) {
  const out = Buffer.alloc(rgba.length);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const shiftedX = axis === 0 ? (x + Math.floor(width / 2)) % width : x;
      const shiftedY = axis === 1 ? (y + Math.floor(height / 2)) % height : y;
      const weight = axis === 0 ? edgeWeight(x, width) : edgeWeight(y, height);
      const own = (y * width + x) * 4;
      const shifted = (shiftedY * width + shiftedX) * 4;
      for (let c = 0; c < 4; c++) out[own + c] = Math.round(rgba[own + c] * weight + rgba[shifted + c] * (1 - weight));
    }
  }
  return out;
}

async function main() {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      cells: { type: "string", default: "4" },
      "cell-px": { type: "string", default: "96" },
    },
  });
  if (positionals.length !== 2) throw new Error(USAGE);
  const [input, output] = positionals;
  const cells = positiveInt("cells", values.cells);
  const size = cells * positiveInt("cell-px", values["cell-px"]);
  const { width, height } = await sharp(input).metadata();
  if (width !== height) throw new Error(`картинка должна быть квадратной, а она ${width}×${height}`);

  // Сначала уменьшение, потом смешивание: уменьшение не знает, что плитка повторяется, и портит
  // крайние точки, а смешивание заменяет их серединой.
  const { data } = await sharp(input).resize(size, size, { kernel: "lanczos3" }).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  const seamless = blendWithHalfShift(blendWithHalfShift(data, size, size, 0), size, size, 1);
  await sharp(seamless, { raw: { width: size, height: size, channels: 4 } }).png().toFile(output);
  console.log(`плитка ${size}×${size}`);
  console.log(`"frames": ${cells * cells}, "columns": ${cells}`);
}

// Тест импортирует чистые функции этого файла, и импорт не должен запускать обработку.
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
