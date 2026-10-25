// Вклеивает тень в видео игры героя (сверху цвет, снизу маска — выход `sheet.mjs`): пятно под каждой ступнёй на её
// высоте и тень фигуры на земле влево и чуть к зрителю, как у камней фона начального экрана «Распутья», где солнце
// справа и чуть сзади. Тень считается по маске каждого кадра и шагает вместе с героем. Кадр расширяется влево и вниз
// под тень. С `--reverse` рядом пишется `<выход>-reverse.mp4` — те же кадры задом наперёд для видео на панели.
// `--unit` — сколько точек выхода в одной точке фона экрана: размеры пятна и размытия заданы в точках фона.
// Запуск: node tools/art/shadows.mjs <вход.mp4> <выход.mp4> [--scale S] [--unit U] [--reverse]

import { linkSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import sharp from "sharp";
import { encodeVideo, readFps, REVERSE_KEYFRAME_SECONDS, reversePath, runFfmpeg, stackColorAndMask } from "./sheet.mjs";

const USAGE = "node tools/art/shadows.mjs <вход.mp4> <выход.mp4> [--scale S] [--unit U] [--reverse]";
// Тень точки фигуры уходит влево на такую долю её высоты над землёй и к зрителю — на такую.
const LEAN_X = 0.55;
const LEAN_Y = 0.2;
// Наибольшая плотность тени фигуры и пятна под ступнёй.
const CAST = 0.7;
const CONTACT = 0.8;
// Цвет тени — тёмно-бурый, как тени на дороге фона.
const SHADOW_COLOR = [0x2a, 0x1a, 0x0e];
// Размеры в точках фона: размытие тени фигуры и пятна, пятно шире ступни влево и вправо, выше и ниже подошвы.
const CAST_BLUR = 3;
const CONTACT_BLUR = 3;
const CONTACT_LEFT = 14;
const CONTACT_RIGHT = 4;
const CONTACT_UP = 2;
const CONTACT_DOWN = 7;
// Ступня — нижняя доля фигуры: пятно ложится под нижнюю точку каждого столбца в этой доле.
const FOOT_SHARE = 0.12;
const SOLID = 128;

/** Нижняя и верхняя строки, где маска сплошная, или `null`, если сплошных точек нет. */
export function figureRows(alpha, width, height) {
  let top = -1;
  let bottom = -1;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (alpha[y * width + x] <= SOLID) continue;
      if (top < 0) top = y;
      bottom = y;
      break;
    }
  }
  return top < 0 ? null : { top, bottom };
}

function boxBlur(src, dst, width, height, radius) {
  const tmp = new Float32Array(width * height);
  const span = 2 * radius + 1;
  for (let y = 0; y < height; y++) {
    let acc = 0;
    for (let x = -radius; x <= radius; x++) acc += src[y * width + Math.min(width - 1, Math.max(0, x))];
    for (let x = 0; x < width; x++) {
      tmp[y * width + x] = acc / span;
      acc += src[y * width + Math.min(width - 1, x + radius + 1)] - src[y * width + Math.max(0, x - radius)];
    }
  }
  for (let x = 0; x < width; x++) {
    let acc = 0;
    for (let y = -radius; y <= radius; y++) acc += tmp[Math.min(height - 1, Math.max(0, y)) * width + x];
    for (let y = 0; y < height; y++) {
      dst[y * width + x] = acc / span;
      acc += tmp[Math.min(height - 1, y + radius + 1) * width + x] - tmp[Math.max(0, y - radius) * width + x];
    }
  }
}

/** Три прохода размытия прямоугольником — почти гауссово размытие. */
function blur(values, width, height, radius) {
  if (radius < 1) return values;
  const a = Float32Array.from(values);
  const b = new Float32Array(width * height);
  boxBlur(a, b, width, height, radius);
  boxBlur(b, a, width, height, radius);
  boxBlur(a, b, width, height, radius);
  return b;
}

/**
 * Плотность тени 0..1 для маски фигуры `alpha` (`width` × `height`), уже стоящей в кадре с запасом под тень.
 * `unit` — точек кадра в одной точке фона. Тень фигуры: точка на высоте `h` над нижней строкой фигуры ложится на
 * `LEAN_X · h` левее и `LEAN_Y · h` ниже. Пятно: под нижней сплошной точкой каждого столбца в нижней доле фигуры.
 */
export function shadowDensity(alpha, width, height, unit) {
  const rows = figureRows(alpha, width, height);
  const density = new Float32Array(width * height);
  if (!rows) return density;
  const feet = rows.bottom;
  const tall = Math.max(1, feet - rows.top);
  const cast = new Float32Array(width * height);
  const contact = new Float32Array(width * height);
  for (let y = rows.top; y <= feet; y++) {
    for (let x = 0; x < width; x++) {
      const a = alpha[y * width + x] / 255;
      if (a < 0.1) continue;
      const above = feet - y;
      const sx = Math.round(x - LEAN_X * above);
      const sy = Math.round(feet + LEAN_Y * above);
      if (sx < 0 || sx >= width || sy >= height) continue;
      const j = sy * width + sx;
      if (a > cast[j]) cast[j] = a;
    }
  }
  const left = Math.round(CONTACT_LEFT * unit);
  const right = Math.round(CONTACT_RIGHT * unit);
  const up = Math.round(CONTACT_UP * unit);
  const down = Math.round(CONTACT_DOWN * unit);
  for (let x = 0; x < width; x++) {
    let low = -1;
    for (let y = feet; y >= Math.max(0, feet - tall * FOOT_SHARE); y--) {
      if (alpha[y * width + x] > SOLID) {
        low = y;
        break;
      }
    }
    if (low < 0) continue;
    for (let cx = Math.max(0, x - left); cx <= Math.min(width - 1, x + right); cx++) {
      for (let cy = Math.max(0, low - up); cy <= Math.min(height - 1, low + down); cy++) contact[cy * width + cx] = 1;
    }
  }
  const castBlur = blur(cast, width, height, Math.round(CAST_BLUR * unit));
  const contactBlur = blur(contact, width, height, Math.round(CONTACT_BLUR * unit));
  for (let j = 0; j < density.length; j++) {
    const fromCast = CAST * Math.min(1, castBlur[j] * 1.2);
    const fromContact = CONTACT * Math.min(1, contactBlur[j] * 1.3);
    density[j] = 1 - (1 - fromCast) * (1 - fromContact);
  }
  return density;
}

/** Герой поверх тени: RGBA кадра, где точки без героя — цвет тени с её плотностью. */
export function composeOverShadow(rgba, density) {
  const out = Buffer.alloc(rgba.length);
  for (let i = 0; i < density.length; i++) {
    const a = rgba[i * 4 + 3] / 255;
    const s = density[i] * (1 - a);
    const total = a + s;
    for (let c = 0; c < 3; c++) {
      out[i * 4 + c] = total > 0 ? Math.round((rgba[i * 4 + c] * a + SHADOW_COLOR[c] * s) / total) : SHADOW_COLOR[c];
    }
    out[i * 4 + 3] = Math.round(255 * total);
  }
  return out;
}

const even = (value) => Math.max(2, Math.round(value / 2) * 2);

function ffmpeg(args, what) {
  const run = runFfmpeg(["-hide_banner", "-loglevel", "error", ...args]);
  if (run.status !== 0) throw new Error(`ffmpeg не ${what}: ${run.stderr.trim().split("\n").pop()}`);
}

async function bake(input, output, { scale, unit, reverse }) {
  const dir = mkdtempSync(join(tmpdir(), "kuznya-shadows-"));
  try {
    const fps = readFps(input);
    const probe = await sharp(await firstFrame(input, dir)).metadata();
    const width = even(probe.width * scale);
    const height = even((probe.height / 2) * scale);
    ffmpeg(["-i", input, "-vf", `scale=${width}:${height * 2}:flags=lanczos`, join(dir, "in%06d.png")], "разрезал видео");
    const inputs = readdirSync(dir).filter((name) => name.startsWith("in")).sort();
    const frames = [];
    let tallest = 0;
    for (const name of inputs) {
      const raw = await sharp(join(dir, name)).removeAlpha().raw().toBuffer();
      const rgba = Buffer.alloc(width * height * 4);
      for (let i = 0; i < width * height; i++) {
        raw.copy(rgba, i * 4, i * 3, i * 3 + 3);
        rgba[i * 4 + 3] = raw[(width * height + i) * 3];
      }
      const alpha = Uint8Array.from({ length: width * height }, (_, i) => rgba[i * 4 + 3]);
      const rows = figureRows(alpha, width, height);
      if (rows) tallest = Math.max(tallest, rows.bottom - rows.top);
      frames.push(rgba);
    }
    const margin = Math.ceil(4 * Math.max(CAST_BLUR, CONTACT_BLUR) * unit);
    const padLeft = even(LEAN_X * tallest + CONTACT_LEFT * unit + margin);
    const padDown = even(LEAN_Y * tallest + CONTACT_DOWN * unit + margin);
    const outWidth = width + padLeft;
    const outHeight = height + padDown;
    for (let k = 0; k < frames.length; k++) {
      const placed = Buffer.alloc(outWidth * outHeight * 4);
      for (let y = 0; y < height; y++) frames[k].copy(placed, (y * outWidth + padLeft) * 4, y * width * 4, (y + 1) * width * 4);
      const alpha = Uint8Array.from({ length: outWidth * outHeight }, (_, i) => placed[i * 4 + 3]);
      const composed = composeOverShadow(placed, shadowDensity(alpha, outWidth, outHeight, unit));
      await sharp(stackColorAndMask(composed, outWidth, outHeight), { raw: { width: outWidth, height: outHeight * 2, channels: 3 } })
        .png({ compressionLevel: 1 })
        .toFile(join(dir, `v${String(k + 1).padStart(6, "0")}.png`));
      if (reverse) linkSync(join(dir, `v${String(k + 1).padStart(6, "0")}.png`), join(dir, `r${String(frames.length - k).padStart(6, "0")}.png`));
    }
    // Ключевые кадры — как у обратного видео программы нарезки: переход видео панели на середину быстрый.
    encodeVideo(join(dir, "v%06d.png"), output, fps, REVERSE_KEYFRAME_SECONDS);
    if (reverse) encodeVideo(join(dir, "r%06d.png"), reversePath(output), fps, REVERSE_KEYFRAME_SECONDS);
    console.log(`кадр ${outWidth}×${outHeight} (запас под тень: слева ${padLeft}, снизу ${padDown}), видео ${outWidth}×${outHeight * 2}, кадров ${frames.length}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

async function firstFrame(input, dir) {
  const file = join(dir, "first.png");
  ffmpeg(["-i", input, "-frames:v", "1", file], "достал первый кадр");
  return file;
}

function positive(name, text, fallback) {
  if (text === undefined) return fallback;
  const value = Number(text);
  if (!(value > 0)) throw new Error(`--${name} должно быть числом больше нуля, а не «${text}»`);
  return value;
}

async function main() {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      scale: { type: "string" },
      unit: { type: "string" },
      reverse: { type: "boolean", default: false },
    },
  });
  if (positionals.length !== 2 || !/\.mp4$/i.test(positionals[1])) throw new Error(USAGE);
  await bake(positionals[0], positionals[1], {
    scale: positive("scale", values.scale, 1),
    unit: positive("unit", values.unit, 1),
    reverse: values.reverse,
  });
}

// Тест импортирует чистые функции этого файла, и импорт не должен запускать вклейку.
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
