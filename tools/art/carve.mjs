// Высекает надписи в камне фона и для каждой рисует те же буквы, светящиеся золотом, — начальный экран «Распутья»:
// строки камня высечены в картинке фона, а горящая строка — отдельная картинка поверх резьбы, буква в букву.
// Буквы — маска шрифта (ffmpeg drawtext). Резьба: внутри буквы камень темнеет; солнце фона справа и сверху, поэтому
// стенка у правого и верхнего края буквы в тени, у левого и нижнего — светлее. Фактура камня видна сквозь резьбу.
// Описание надписей — JSON: { "font": путь, "lines": [{ "id", "text": [строки], "cx", "top", "size", "step" }] },
// места и размеры — в точках фона. Пишет <папка>/<фон>, glow_<id>.png и glow.json с прямоугольниками свечений.
// Запуск: node tools/art/carve.mjs <фон.png> <надписи.json> <папка>

import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import ffmpegPath from "ffmpeg-static";
import sharp from "sharp";

// Насколько темнеет камень внутри резьбы, стенка в тени и светлая стенка.
const GROOVE = 0.48;
const SHADE = 0.45;
const LIGHT = 0.3;
// Сдвиг к солнцу, по которому находятся стенки, — в долях размера шрифта.
const WALL = 0.05;
// Свечение: цвет букв, цвет ореола, размытие ореола в долях размера шрифта и его плотность.
const LETTER = [0xff, 0xe0, 0x8a];
const HALO = [0xff, 0xb8, 0x40];
const HALO_BLUR = 0.18;
const HALO_DENSITY = 0.75;
const MARGIN = 0.6;

/** Маска строк надписи — серый слой размера фона, белое — буква. */
function renderMask(width, height, font, line) {
  const fontPath = resolve(font).replace(/\\/g, "/").replace(/:/g, "\\:");
  const draws = line.text.map((text, i) => {
    const y = line.top + i * line.step;
    return `drawtext=fontfile='${fontPath}':text='${text.replace(/'/g, "’")}':fontsize=${line.size}:fontcolor=white:x=${line.cx}-text_w/2:y=${y}`;
  });
  // Маска — байты кадра на выходе ffmpeg, поэтому вывод читается буфером, а не текстом. Кадр в RGB: серый слой
  // ffmpeg хранит чёрное как 16, а в RGB чёрное — точный 0, и резьба не трогает камень вокруг букв.
  const run = spawnSync(ffmpegPath, [
    "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", `color=c=black:s=${width}x${height}`,
    "-vf", ["format=rgb24", ...draws].join(","), "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
  ], { maxBuffer: width * height * 4 });
  if (run.error) throw new Error(`ffmpeg не запустился: ${run.error.message}`);
  if (run.status !== 0) throw new Error(`ffmpeg не нарисовал надпись «${line.id}»: ${run.stderr.toString().trim().split("\n").pop()}`);
  const mask = Buffer.alloc(width * height);
  for (let i = 0; i < mask.length; i++) mask[i] = run.stdout[i * 3];
  return mask;
}

/** Резьба по маске `mask` (0..255) в RGB фона `rgb`; `wall` — сдвиг к солнцу в точках. */
export function carve(rgb, mask, width, height, wall) {
  const at = (x, y) => (x < 0 || y < 0 || x >= width || y >= height ? 0 : mask[y * width + x] / 255);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const m = at(x, y);
      if (m === 0) continue;
      const shade = Math.max(0, m - at(x + wall, y - wall));
      const light = Math.max(0, m - at(x - wall, y + wall));
      const factor = (1 - GROOVE * m) * (1 - SHADE * shade) * (1 + LIGHT * light);
      const i = (y * width + x) * 3;
      for (let c = 0; c < 3; c++) rgb[i + c] = Math.min(255, Math.round(rgb[i + c] * factor));
    }
  }
}

function blurMask(values, width, height, radius) {
  const tmp = new Float32Array(width * height);
  const out = new Float32Array(width * height);
  const span = 2 * radius + 1;
  for (let pass = 0; pass < 3; pass++) {
    const src = pass === 0 ? values : out;
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
        out[y * width + x] = acc / span;
        acc += tmp[Math.min(height - 1, y + radius + 1) * width + x] - tmp[Math.max(0, y - radius) * width + x];
      }
    }
  }
  return out;
}

/** Светящиеся буквы надписи: RGBA прямоугольника `box` — буквы золотом и мягкий ореол вокруг. */
export function glow(mask, width, box, size) {
  const local = new Float32Array(box.width * box.height);
  for (let y = 0; y < box.height; y++) {
    for (let x = 0; x < box.width; x++) local[y * box.width + x] = mask[(box.top + y) * width + box.left + x] / 255;
  }
  const halo = blurMask(local, box.width, box.height, Math.max(1, Math.round(size * HALO_BLUR)));
  const rgba = Buffer.alloc(box.width * box.height * 4);
  for (let i = 0; i < local.length; i++) {
    const letter = local[i];
    const ring = Math.min(1, halo[i] * 2) * HALO_DENSITY * (1 - letter);
    const alpha = letter + ring;
    for (let c = 0; c < 3; c++) rgba[i * 4 + c] = alpha > 0 ? Math.round((LETTER[c] * letter + HALO[c] * ring) / alpha) : HALO[c];
    rgba[i * 4 + 3] = Math.round(255 * Math.min(1, alpha));
  }
  return rgba;
}

function maskBox(mask, width, height, margin) {
  let left = width;
  let top = height;
  let right = -1;
  let bottom = -1;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (mask[y * width + x] === 0) continue;
      left = Math.min(left, x);
      right = Math.max(right, x);
      top = Math.min(top, y);
      bottom = Math.max(bottom, y);
    }
  }
  if (right < 0) throw new Error("в надписи нет ни одной буквы");
  const l = Math.max(0, left - margin);
  const t = Math.max(0, top - margin);
  return { left: l, top: t, width: Math.min(width - 1, right + margin) - l + 1, height: Math.min(height - 1, bottom + margin) - t + 1 };
}

async function main() {
  const [background, specFile, folder] = process.argv.slice(2);
  if (!folder) throw new Error("node tools/art/carve.mjs <фон.png> <надписи.json> <папка>");
  const spec = JSON.parse(readFileSync(specFile, "utf8"));
  const { data: rgb, info } = await sharp(background).removeAlpha().raw().toBuffer({ resolveWithObject: true });
  const boxes = {};
  for (const line of spec.lines) {
    const mask = renderMask(info.width, info.height, spec.font, line);
    carve(rgb, mask, info.width, info.height, Math.max(1, Math.round(line.size * WALL)));
    const box = maskBox(mask, info.width, info.height, Math.round(line.size * MARGIN));
    await sharp(glow(mask, info.width, box, line.size), { raw: { width: box.width, height: box.height, channels: 4 } })
      .png()
      .toFile(join(folder, `glow_${line.id}.png`));
    boxes[line.id] = box;
  }
  await sharp(rgb, { raw: { width: info.width, height: info.height, channels: 3 } }).png().toFile(join(folder, basename(background)));
  writeFileSync(join(folder, "glow.json"), JSON.stringify({ width: info.width, height: info.height, boxes }, null, 2));
  console.log(`резьба: ${spec.lines.map((line) => line.id).join(", ")}; свечения и glow.json — в ${folder}`);
}

// Тест импортирует чистые функции этого файла, и импорт не должен запускать резьбу.
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
