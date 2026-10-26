// Видео имени героя начального экрана «Распутья»: имя проявляется по буквам тем же волшебством, каким лягушка
// становится Василисой. Над буквой вспыхивает светлая дымка, снизу вверх по букве идёт полоса золотого света, за ней
// остаётся раскалённая буква, она остывает до золота горящих строк камня, а дымка рассеивается рваным краем и роняет
// искры. Буквы — маска шрифта (ffmpeg drawtext); маска буквы — разница строки до неё и строки с ней, поэтому
// расстояния между буквами те же, что у целой строки. Последний кадр — готовое имя без дымки и искр: видео панели
// под мышью стоит на нём, а когда мышь ушла, играет назад, и имя гаснет. Строки имени разделяет `|`.
// Запуск: node tools/art/materialize.mjs <шрифт.ttf> <выход.mp4> --text "Илья|Муромец" [--size 96] [--delay 0]
//   [--reverse]

import { linkSync, mkdtempSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import ffmpegPath from "ffmpeg-static";
import sharp from "sharp";
import { bleedColor, encodeVideo, REVERSE_KEYFRAME_SECONDS, reversePath, smoothstep, stackColorAndMask } from "./sheet.mjs";

const USAGE = 'node tools/art/materialize.mjs <шрифт.ttf> <выход.mp4> --text "Илья|Муромец" [--size 96] [--delay 0] [--reverse]';
const FPS = 24;
// Время одной буквы от вспышки до остывшего золота, шаг между буквами и запас в конце, когда искры догорают.
const LETTER_TIME = 0.9;
const LETTER_STEP = 0.08;
const TAIL = 0.7;
// Шаг строк и поля кадра в долях размера шрифта: сверху больше — туда поднимаются искры.
const LINE_STEP = 1.1;
const MARGIN_SIDE = 0.6;
const MARGIN_TOP = 0.9;
const MARGIN_BOTTOM = 0.5;
// Цвета: раскалённая буква, остывшая (как у горящей строки камня), ореол, дымка, полоса света, искра, тень под буквами.
const HOT = [0xff, 0xfb, 0xe8];
const LETTER = [0xff, 0xe0, 0x8a];
const HALO = [0xff, 0xb8, 0x40];
const HAZE = [0xe6, 0xe2, 0xda];
const BAND = [0xff, 0xc8, 0x50];
const SPARK = [0xff, 0xea, 0xa8];
const SHADE = [0x1e, 0x12, 0x08];
// Размытия в долях размера шрифта и плотности: ореол, тень (и её сдвиг влево-вниз — солнце фона справа), дымка.
const HALO_BLUR = 0.18;
const HALO_DENSITY = 0.75;
const SHADE_BLUR = 0.1;
const SHADE_DENSITY = 0.6;
const SHADE_SHIFT = 0.03;
const HAZE_BLUR = 0.3;
const HAZE_DENSITY = 0.55;
// Доля точек дымки с золотой пылинкой — выше этого порога случайного числа.
const DUST = 0.985;
// Полоса света: толщина и насколько она шире буквы с каждой стороны, в долях размера шрифта; искр на букву и их
// подъём в размерах шрифта за секунду.
const BAND_WIDTH = 0.08;
const BAND_REACH = 0.3;
const SPARKS = 9;
const SPARK_RISE = 0.45;
// Рваный край дымки: размер пятен шума в долях размера шрифта и ширина края.
const NOISE_SCALE = 0.16;
const RAGGED = 0.15;

/**
 * Ход одной буквы в момент `t` секунд от её начала, всё 0..1: дымка `haze` вспыхивает и рассеивается, полоса света
 * поднимается `reveal` снизу вверх, `cool` — от раскалённого цвета к золоту, `glow` — ореол и тень готовой буквы.
 */
export function letterPhase(t) {
  const d = LETTER_TIME;
  return {
    grow: smoothstep(0, 0.15 * d, t),
    dissolve: smoothstep(0.4 * d, 0.95 * d, t),
    reveal: smoothstep(0.15 * d, 0.55 * d, t),
    cool: smoothstep(0.45 * d, d, t),
    glow: smoothstep(0.3 * d, d, t),
  };
}

/** Длина видео в секундах: задержка, все буквы и запас на догорание искр. */
export function duration(letters, delay) {
  return delay + Math.max(0, letters - 1) * LETTER_STEP + LETTER_TIME + TAIL;
}

/** Маски букв из масок строк до буквы и с ней: точки, которые добавила буква. */
export function letterMasks(prefixes) {
  const masks = [];
  for (let k = 1; k < prefixes.length; k++) {
    const mask = new Float32Array(prefixes[k].length);
    for (let i = 0; i < mask.length; i++) mask[i] = Math.max(0, prefixes[k][i] - prefixes[k - 1][i]) / 255;
    masks.push(mask);
  }
  return masks;
}

/** Слой поверх кадра в цвете с долей покрытия: `frame` — RGB, умноженный на прозрачность, и прозрачность. */
export function over(frame, i, color, cover) {
  if (cover <= 0) return;
  const a = Math.min(1, cover);
  for (let c = 0; c < 3; c++) frame[i * 4 + c] = color[c] * a + frame[i * 4 + c] * (1 - a);
  frame[i * 4 + 3] = a + frame[i * 4 + 3] * (1 - a);
}

function random(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = state;
    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    return ((value ^ (value >>> 14)) >>> 0) / 4294967296;
  };
}

function hash(x, y) {
  let h = Math.imul(x, 374761393) + Math.imul(y, 668265263);
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

/** Гладкий шум 0..1 в точке, две октавы. */
function noise(x, y) {
  const octave = (px, py) => {
    const x0 = Math.floor(px);
    const y0 = Math.floor(py);
    const fx = px - x0;
    const fy = py - y0;
    const sx = fx * fx * (3 - 2 * fx);
    const sy = fy * fy * (3 - 2 * fy);
    const top = hash(x0, y0) + (hash(x0 + 1, y0) - hash(x0, y0)) * sx;
    const bottom = hash(x0, y0 + 1) + (hash(x0 + 1, y0 + 1) - hash(x0, y0 + 1)) * sx;
    return top + (bottom - top) * sy;
  };
  return 0.65 * octave(x, y) + 0.35 * octave(x * 2.3 + 17, y * 2.3 + 5);
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
  if (radius < 1) return Float32Array.from(values);
  const a = Float32Array.from(values);
  const b = new Float32Array(width * height);
  boxBlur(a, b, width, height, radius);
  boxBlur(b, a, width, height, radius);
  boxBlur(a, b, width, height, radius);
  return b;
}

/** Маска строки текста в кадре `width` × `height`, 0..255; `x`, `y` — левый верхний угол строки. */
function renderText(font, text, size, width, height, x, y) {
  const fontPath = resolve(font).replace(/\\/g, "/").replace(/:/g, "\\:");
  const draw = text === "" ? "null" : `drawtext=fontfile='${fontPath}':text='${text.replace(/'/g, "’")}':fontsize=${size}:fontcolor=white:x=${x}:y=${y}`;
  // Маска — байты кадра на выходе ffmpeg, поэтому вывод читается буфером. Кадр в RGB: чёрное в нём — точный 0.
  const run = spawnSync(ffmpegPath, [
    "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", `color=c=black:s=${width}x${height}`,
    "-vf", `format=rgb24,${draw}`, "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
  ], { maxBuffer: width * height * 4 });
  if (run.error) throw new Error(`ffmpeg не запустился: ${run.error.message}`);
  if (run.status !== 0) throw new Error(`ffmpeg не нарисовал «${text}»: ${run.stderr.toString().trim().split("\n").pop()}`);
  const mask = new Uint8Array(width * height);
  for (let i = 0; i < mask.length; i++) mask[i] = run.stdout[i * 3];
  return mask;
}

function bounds(mask, width, height) {
  let left = width;
  let right = -1;
  let top = height;
  let bottom = -1;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (mask[y * width + x] < 8) continue;
      left = Math.min(left, x);
      right = Math.max(right, x);
      top = Math.min(top, y);
      bottom = Math.max(bottom, y);
    }
  }
  return right < 0 ? null : { left, right, top, bottom };
}

const even = (value) => Math.max(2, Math.ceil(value / 2) * 2);

/** Буквы имени в кадре: маска, её рамка и искры каждой буквы; размер кадра. */
function layout(font, lines, size) {
  // Сначала каждая строка целиком в просторном кадре — по ней видно ширину строки и высоту букв над её верхом.
  const probeWidth = Math.ceil(size * (Math.max(...lines.map((line) => line.length)) + 2));
  const probeHeight = Math.ceil(size * 3);
  const measured = lines.map((line) => bounds(renderText(font, line, size, probeWidth, probeHeight, size, size), probeWidth, probeHeight));
  if (measured.some((box) => box === null)) throw new Error("в строке имени нет ни одной буквы шрифта");
  const widest = Math.max(...measured.map((box) => box.right - box.left + 1));
  const width = even(widest + 2 * MARGIN_SIDE * size);
  const inkTop = Math.min(...measured.map((box) => box.top)) - size;
  const inkBottom = Math.max(...measured.map((box) => box.bottom)) - size;
  const height = even(MARGIN_TOP * size + (lines.length - 1) * LINE_STEP * size + (inkBottom - inkTop + 1) + MARGIN_BOTTOM * size);
  const letters = [];
  lines.forEach((line, row) => {
    const box = measured[row];
    const x = Math.round((width - (box.right - box.left + 1)) / 2 - (box.left - size));
    const y = Math.round(MARGIN_TOP * size + row * LINE_STEP * size - inkTop);
    const prefixes = [];
    for (let k = 0; k <= line.length; k++) prefixes.push(renderText(font, line.slice(0, k), size, width, height, x, y));
    letterMasks(prefixes).forEach((mask) => {
      const box = bounds(mask.map((value) => value * 255), width, height);
      if (box) letters.push({ mask, box });
    });
  });
  return { width, height, letters };
}

function sparksOf(letter, size, rand) {
  const { box } = letter;
  return Array.from({ length: SPARKS }, () => ({
    x: box.left + rand() * (box.right - box.left) + (rand() - 0.5) * 0.3 * size,
    y: box.top + rand() * (box.bottom - box.top),
    born: (0.15 + 0.5 * rand()) * LETTER_TIME,
    life: 0.45 + 0.4 * rand(),
    rise: SPARK_RISE * size * (0.6 + 0.8 * rand()),
    drift: (rand() - 0.5) * 0.25 * size,
    radius: size * (0.012 + 0.016 * rand()),
    twinkle: rand() * Math.PI * 2,
  }));
}

/** Имя в кадрах: RGBA каждого кадра. */
function* frames(name, size, delay) {
  const { width, height, letters } = name;
  const total = width * height;
  const halo = new Float32Array(total);
  const shadeMask = new Float32Array(total);
  const rand = random(letters.length * 7919 + width);
  const shift = Math.round(SHADE_SHIFT * size);
  // Для каждой буквы заранее: ореол, тень и основа дымки — размытые маски буквы.
  const prepared = letters.map((letter) => {
    for (let i = 0; i < total; i++) halo[i] = letter.mask[i];
    const ownHalo = blur(halo, width, height, Math.round(HALO_BLUR * size / 2));
    shadeMask.fill(0);
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const sx = x + shift;
        const sy = y - shift;
        if (sx < width && sy >= 0) shadeMask[y * width + x] = letter.mask[sy * width + sx];
      }
    }
    const shade = blur(shadeMask, width, height, Math.round(SHADE_BLUR * size / 2));
    const haze = blur(letter.mask, width, height, Math.round(HAZE_BLUR * size / 2));
    for (let i = 0; i < total; i++) haze[i] = Math.min(1, haze[i] * 2);
    return { ...letter, halo: ownHalo, shade, haze, sparks: sparksOf(letter, size, rand) };
  });
  const count = Math.round(duration(letters.length, delay) * FPS);
  for (let f = 0; f < count; f++) {
    const time = f / FPS;
    const frame = new Float32Array(total * 4);
    const phases = prepared.map((_, k) => letterPhase(time - delay - k * LETTER_STEP));
    // Тень и ореол готовых букв — под всем остальным.
    for (let i = 0; i < total; i++) {
      let shade = 0;
      let glow = 0;
      prepared.forEach((letter, k) => {
        shade = Math.max(shade, letter.shade[i] * phases[k].glow);
        glow = Math.max(glow, letter.halo[i] * phases[k].glow);
      });
      over(frame, i, SHADE, SHADE_DENSITY * Math.min(1, shade * 1.6));
      over(frame, i, HALO, HALO_DENSITY * Math.min(1, glow * 1.4));
    }
    prepared.forEach((letter, k) => {
      const phase = phases[k];
      if (phase.grow <= 0) return;
      const { box } = letter;
      const reach = Math.round(HAZE_BLUR * size * 1.5);
      const left = Math.max(0, box.left - reach);
      const right = Math.min(width - 1, box.right + reach);
      const top = Math.max(0, box.top - reach);
      const bottom = Math.min(height - 1, box.bottom + reach);
      const tall = box.bottom - box.top + 1;
      const band = BAND_WIDTH * size;
      const line = box.bottom + band - phase.reveal * (tall + 2 * band);
      const color = HOT.map((hot, c) => hot + (LETTER[c] - hot) * phase.cool);
      const drift = (time - delay - k * LETTER_STEP) * 0.6;
      for (let y = top; y <= bottom; y++) {
        for (let x = left; x <= right; x++) {
          const i = y * width + x;
          // Дымка: светлое облако вокруг буквы, рвётся по шуму и тает, пока шум не обгонит её плотность.
          if (phase.dissolve < 1) {
            const n = noise(x / (NOISE_SCALE * size) + k * 13.1, y / (NOISE_SCALE * size) - drift);
            const edge = letter.haze[i] * phase.grow - (0.12 + 0.88 * phase.dissolve) * (0.3 + 0.7 * n);
            const cover = Math.min(1, Math.max(0, edge / RAGGED));
            over(frame, i, HAZE, HAZE_DENSITY * cover);
            // Золотая пыль в дымке: мелкие мерцающие крупинки.
            if (cover > 0 && hash(x + f * 7919, y) > DUST) over(frame, i, LETTER, cover * 0.8);
          }
          const m = letter.mask[i];
          // Буква — ниже поднимающейся полосы, как Василиса под полосой света.
          const shown = Math.min(1, Math.max(0, (y - line) / band + 0.5));
          over(frame, i, color, m * shown);
        }
      }
      // Полоса света — поперёк буквы и шире её, гаснет к концам; сердцевина почти белая, вокруг золото.
      if (phase.reveal <= 0 || phase.reveal >= 1) return;
      const strength = Math.sin(Math.PI * phase.reveal);
      const half = (box.right - box.left) / 2 + BAND_REACH * size;
      const middle = (box.left + box.right) / 2;
      for (let y = Math.max(0, Math.floor(line - 2 * band)); y <= Math.min(height - 1, Math.ceil(line + 2 * band)); y++) {
        for (let x = Math.max(0, Math.floor(middle - half)); x <= Math.min(width - 1, Math.ceil(middle + half)); x++) {
          const across = 1 - ((x - middle) / half) ** 2;
          const off = (y - line) / band;
          over(frame, y * width + x, BAND, strength * across * 0.85 * Math.exp(-off * off * 1.5));
          over(frame, y * width + x, HOT, strength * across * Math.exp(-off * off * 12));
        }
      }
    });
    // Искры: точка с лучами крестом, поднимается и мерцает.
    prepared.forEach((letter, k) => {
      const local = time - delay - k * LETTER_STEP;
      for (const spark of letter.sparks) {
        const age = local - spark.born;
        if (age < 0 || age > spark.life) continue;
        const fade = Math.sin((Math.PI * age) / spark.life) * (0.65 + 0.35 * Math.sin(spark.twinkle + age * 25));
        const cx = spark.x + spark.drift * age;
        const cy = spark.y - spark.rise * age;
        const r = spark.radius;
        const ray = r * 5;
        for (let y = Math.max(0, Math.floor(cy - ray)); y <= Math.min(height - 1, Math.ceil(cy + ray)); y++) {
          for (let x = Math.max(0, Math.floor(cx - ray)); x <= Math.min(width - 1, Math.ceil(cx + ray)); x++) {
            const dx = x - cx;
            const dy = y - cy;
            const dot = Math.exp(-(dx * dx + dy * dy) / (r * r));
            const cross = Math.exp(-Math.abs(dx * dy) / (r * r * 0.3)) * Math.exp(-Math.hypot(dx, dy) / ray * 2.5) * 0.7;
            over(frame, y * width + x, SPARK, fade * Math.min(1, dot + cross));
          }
        }
      }
    });
    const rgba = Buffer.alloc(total * 4);
    for (let i = 0; i < total; i++) {
      const a = frame[i * 4 + 3];
      for (let c = 0; c < 3; c++) rgba[i * 4 + c] = a > 0 ? Math.round(Math.min(255, frame[i * 4 + c] / a)) : 0;
      rgba[i * 4 + 3] = Math.round(255 * a);
    }
    yield rgba;
  }
}

async function materialize(font, output, { text, size, delay, reverse }) {
  const lines = text.split("|");
  const name = layout(font, lines, size);
  const dir = mkdtempSync(join(tmpdir(), "kuznya-name-"));
  try {
    const count = Math.round(duration(name.letters.length, delay) * FPS);
    let k = 0;
    for (const rgba of frames(name, size, delay)) {
      bleedColor(rgba, name.width, name.height);
      const file = join(dir, `v${String(k + 1).padStart(6, "0")}.png`);
      await sharp(stackColorAndMask(rgba, name.width, name.height), { raw: { width: name.width, height: name.height * 2, channels: 3 } })
        .png({ compressionLevel: 1 })
        .toFile(file);
      if (reverse) linkSync(file, join(dir, `r${String(count - k).padStart(6, "0")}.png`));
      k++;
    }
    encodeVideo(join(dir, "v%06d.png"), output, FPS, REVERSE_KEYFRAME_SECONDS);
    if (reverse) encodeVideo(join(dir, "r%06d.png"), reversePath(output), FPS, REVERSE_KEYFRAME_SECONDS);
    console.log(`кадр ${name.width}×${name.height}, видео ${name.width}×${name.height * 2}, кадров ${count}, ${(count / FPS).toFixed(2)} с`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function number(name, text, fallback, min) {
  if (text === undefined) return fallback;
  const value = Number(text);
  if (!Number.isFinite(value) || value < min) throw new Error(`--${name} должно быть числом от ${min}, а не «${text}»`);
  return value;
}

async function main() {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      text: { type: "string" },
      size: { type: "string" },
      delay: { type: "string" },
      reverse: { type: "boolean", default: false },
    },
  });
  if (positionals.length !== 2 || !/\.mp4$/i.test(positionals[1]) || !values.text) throw new Error(USAGE);
  await materialize(positionals[0], positionals[1], {
    text: values.text,
    size: Math.round(number("size", values.size, 96, 8)),
    delay: number("delay", values.delay, 0, 0),
    reverse: values.reverse,
  });
}

// Тест импортирует чистые функции этого файла, и импорт не должен запускать сборку видео.
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
