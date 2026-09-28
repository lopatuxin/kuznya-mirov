// Превращает видео или картинку нейросети на однотонном ярко-зелёном фоне в PNG с прозрачным
// фоном для игры. Траву, листву и всё зелёное нейросеть рисует на ярко-розовом фоне
// (`--key magenta`): зелёный фон убрал бы их вместе с собой. Картинка вырезается по своему содержимому. Видео становится листом кадров
// сеткой: повторяющийся шаг находится сам, а движение без повтора (удар, падение) задаётся
// номерами кадров через --range. Персонаж в видео должен двигаться на месте при неподвижной
// камере: все кадры режутся одним общим прямоугольником, и если персонаж идёт через кадр, он
// будет перемещаться внутри листа. Зависимости ставятся один раз: `npm install` в `tools/art/`.

import { mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import ffmpegPath from "ffmpeg-static";
import sharp from "sharp";

const VIDEO_EXTENSIONS = new Set([".mp4", ".webm", ".mov"]);
const USAGE = "node tools/art/sheet.mjs <вход.mp4|.png> <выход.png> [--frames N] [--height H] [--columns C] [--range A-B] [--key green|magenta] [--fade-ends P]";
const MARGIN = 4;
const VISIBLE_ALPHA = 26;
const THUMB_SIZE = 96;
const MIN_PERIOD = 4;

function positiveInt(name, text) {
  if (text === undefined) return undefined;
  const value = Number(text);
  if (!Number.isInteger(value) || value < 1) throw new Error(`--${name} должно быть целым числом от 1, а не «${text}»`);
  return value;
}

/** Кадры `A-B` видео, считая с единицы, — в начало и длину, считая с нуля. */
export function parseRange(text) {
  const match = /^(\d+)-(\d+)$/.exec(text);
  const first = match ? Number(match[1]) : 0;
  const last = match ? Number(match[2]) : 0;
  if (!match || first < 1 || last < first) throw new Error(`--range должно быть вида A-B, где 1 ≤ A ≤ B, а не «${text}»`);
  return { start: first - 1, length: last - first + 1 };
}

export function smoothstep(edge0, edge1, x) {
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

/**
 * Точки RGB — в RGBA без зелёного фона. Прозрачность — по тому, насколько зелёный канал выше
 * красного и синего: чистый фон исчезает, персонаж остаётся, край получает промежуточную
 * прозрачность. Зелёный канал прижимается к большему из двух других, чтобы по краю не оставалась
 * зелёная кайма.
 */
export function keyGreen(rgb) {
  const rgba = Buffer.alloc((rgb.length / 3) * 4);
  for (let src = 0, dst = 0; src < rgb.length; src += 3, dst += 4) {
    const red = rgb[src];
    const green = rgb[src + 1];
    const blue = rgb[src + 2];
    const other = Math.max(red, blue);
    rgba[dst] = red;
    rgba[dst + 1] = Math.min(green, other);
    rgba[dst + 2] = blue;
    rgba[dst + 3] = Math.round(255 * (1 - smoothstep(25, 110, green - other)));
  }
  return rgba;
}

/**
 * Точки RGB — в RGBA без ярко-розового фона. Прозрачность — по тому, насколько меньший из красного
 * и синего выше зелёного: у розового высоки оба, у бурой земли и красных цветов — только один.
 * Красный и синий снижаются на этот избыток, чтобы по краю не оставалась розовая кайма.
 */
export function keyMagenta(rgb) {
  const rgba = Buffer.alloc((rgb.length / 3) * 4);
  for (let src = 0, dst = 0; src < rgb.length; src += 3, dst += 4) {
    const red = rgb[src];
    const green = rgb[src + 1];
    const blue = rgb[src + 2];
    const excess = Math.max(0, Math.min(red, blue) - green);
    rgba[dst] = red - excess;
    rgba[dst + 1] = green;
    rgba[dst + 2] = blue - excess;
    rgba[dst + 3] = Math.round(255 * (1 - smoothstep(25, 110, excess)));
  }
  return rgba;
}

const KEYS = { green: keyGreen, magenta: keyMagenta };

async function removeBackground(file, key) {
  const { data, info } = await sharp(file).removeAlpha().raw().toBuffer({ resolveWithObject: true });
  return { rgba: KEYS[key](data), width: info.width, height: info.height };
}

// Один прямоугольник на все кадры: иначе персонаж прыгал бы внутри листа от кадра к кадру.
function visibleBox(images) {
  const { width, height } = images[0];
  let left = width;
  let top = height;
  let right = -1;
  let bottom = -1;
  for (const { rgba } of images) {
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        if (rgba[(y * width + x) * 4 + 3] < VISIBLE_ALPHA) continue;
        left = Math.min(left, x);
        top = Math.min(top, y);
        right = Math.max(right, x);
        bottom = Math.max(bottom, y);
      }
    }
  }
  if (right < 0) throw new Error("после удаления фона ничего не осталось");
  left = Math.max(0, left - MARGIN);
  top = Math.max(0, top - MARGIN);
  right = Math.min(width - 1, right + MARGIN);
  bottom = Math.min(height - 1, bottom + MARGIN);
  return { left, top, width: right - left + 1, height: bottom - top + 1 };
}

function cutOut({ rgba, width, height }, box, targetHeight) {
  const image = sharp(rgba, { raw: { width, height, channels: 4 } }).extract(box);
  return (targetHeight ? image.resize({ height: targetHeight, kernel: "lanczos3" }) : image)
    .png()
    .toBuffer({ resolveWithObject: true });
}

function runFfmpeg(args) {
  if (!ffmpegPath) throw new Error("для этой системы у ffmpeg-static нет готового ffmpeg");
  const run = spawnSync(ffmpegPath, args, { encoding: "utf8" });
  if (run.error) throw new Error(`ffmpeg не запустился: ${run.error.message}`);
  return run;
}

function lastLine(text) {
  return text.trim().split("\n").pop();
}

function readFps(input) {
  const probe = runFfmpeg(["-hide_banner", "-i", input]);
  const match = /(\d+(?:\.\d+)?) fps/.exec(probe.stderr);
  if (!match) throw new Error(`ffmpeg не узнал частоту кадров видео ${input}: ${lastLine(probe.stderr)}`);
  return Number(match[1]);
}

function extractFrames(input, dir) {
  const run = runFfmpeg(["-hide_banner", "-loglevel", "error", "-i", input, "-fps_mode", "passthrough", join(dir, "f%06d.png")]);
  if (run.status !== 0) throw new Error(`ffmpeg не разрезал видео: ${lastLine(run.stderr)}`);
  return readdirSync(dir).filter((name) => name.endsWith(".png")).sort().map((name) => join(dir, name));
}

function meanDifference(a, b) {
  let sum = 0;
  for (let i = 0; i < a.length; i++) sum += Math.abs(a[i] - b[i]);
  return sum / a.length;
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

/**
 * Начало и длина повторяющегося цикла в эскизах кадров. Длина цикла — сдвиг, на котором кадры
 * повторяются: средняя разность кадра и кадра через этот сдвиг резко падает. Кратные сдвиги тоже
 * дают малую разность, поэтому берётся самый короткий местный минимум, заметно ниже обычной
 * разности. Но у ходьбы шаг левой и шаг правой ногой похожи, и минимум бывает уже на половине
 * цикла: если вдвое больший сдвиг совпадает заметно лучше, цикл — он. Первая четверть видео —
 * выход из стойки в движение, в цикл она не берётся.
 */
export function findCycle(thumbs) {
  const skip = Math.floor(thumbs.length / 4);
  const maxPeriod = Math.floor((thumbs.length - skip) / 2);
  const scores = new Map();
  for (let period = MIN_PERIOD; period <= maxPeriod; period++) {
    let sum = 0;
    let count = 0;
    for (let i = skip; i + period < thumbs.length; i++) {
      sum += meanDifference(thumbs[i], thumbs[i + period]);
      count++;
    }
    scores.set(period, sum / count);
  }
  const typical = median([...scores.values()]);
  const isDip = (period) => {
    const score = scores.get(period);
    return score < typical / 2
      && score <= (scores.get(period - 1) ?? Infinity)
      && score <= (scores.get(period + 1) ?? Infinity);
  };
  let period = [...scores.keys()].find(isDip);
  if (period === undefined) throw new Error("в видео не нашлось повторяющегося движения — задай кадры через --range");
  while (scores.has(period * 2) && scores.get(period * 2) < scores.get(period) / 2) period *= 2;

  // Цикл начинается там, где кадр лучше всего совпадает с кадром через цикл: так стык незаметен.
  let start = skip;
  let best = Infinity;
  for (let i = skip; i + period < thumbs.length; i++) {
    const difference = meanDifference(thumbs[i], thumbs[i + period]);
    if (difference < best) {
      best = difference;
      start = i;
    }
  }
  return { start, length: period };
}

/** `count` кадров, равномерно взятых из отрезка `span`. */
export function pickFrames(span, count) {
  return Array.from({ length: count }, (_, k) => span.start + Math.round((k * span.length) / count));
}

async function makeSheet(input, output, options) {
  const dir = mkdtempSync(join(tmpdir(), "kuznya-sheet-"));
  try {
    const fps = readFps(input);
    const files = extractFrames(input, dir);
    let span = options.range;
    if (span && span.start + span.length > files.length) throw new Error(`--range выходит за конец видео (кадров в видео: ${files.length})`);
    if (!span) {
      const thumbs = await Promise.all(files.map((file) =>
        sharp(file).resize(THUMB_SIZE, THUMB_SIZE, { fit: "fill" }).greyscale().raw().toBuffer()));
      span = findCycle(thumbs);
    }
    const count = Math.min(options.frames ?? span.length, span.length);
    const images = await Promise.all(pickFrames(span, count).map((i) => removeBackground(files[i], options.key)));
    const box = visibleBox(images);
    const frames = await Promise.all(images.map((image) => cutOut(image, box, options.height)));
    const { width: frameWidth, height: frameHeight } = frames[0].info;
    const columns = Math.min(options.columns, count);
    const rows = Math.ceil(count / columns);
    await sharp({ create: { width: frameWidth * columns, height: frameHeight * rows, channels: 4, background: { r: 0, g: 0, b: 0, alpha: 0 } } })
      .composite(frames.map((frame, k) => ({ input: frame.data, left: (k % columns) * frameWidth, top: Math.floor(k / columns) * frameHeight })))
      .png()
      .toFile(output);
    console.log(`кадры видео ${span.start + 1}-${span.start + span.length} из ${files.length}, взято ${count}`);
    if (span.length % count !== 0) console.log(`внимание: ${count} не делит ${span.length}, кадры взяты через неравные промежутки`);
    console.log(`кадр ${frameWidth}×${frameHeight}, в строке ${columns}, строк ${rows}, лист ${frameWidth * columns}×${frameHeight * rows}`);
    console.log(`"frames": ${count}, "columns": ${columns}, "frame_time": ${(span.length / count / fps).toFixed(4)}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/**
 * Плавно гасит левый и правый концы куска земли на `fade` точек от крайней видимой точки. Кусок
 * дорожки, заведённый на соседний на удвоенную длину угасания, ложится на него без шва: погашенный
 * конец одного лежит поверх или под непрозрачной серединой другого.
 */
export function fadeEnds(rgba, width, height, fade) {
  let left = width;
  let right = -1;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (rgba[(y * width + x) * 4 + 3] < VISIBLE_ALPHA) continue;
      left = Math.min(left, x);
      right = Math.max(right, x);
    }
  }
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const alpha = (y * width + x) * 4 + 3;
      rgba[alpha] = Math.round(rgba[alpha] * smoothstep(0, fade, x - left) * smoothstep(0, fade, right - x));
    }
  }
}

async function makeCutout(input, output, height, key, fade) {
  const image = await removeBackground(input, key);
  const { data, info } = await cutOut(image, visibleBox([image]), height);
  if (fade === undefined) {
    writeFileSync(output, data);
  } else {
    const raw = await sharp(data).raw().toBuffer();
    fadeEnds(raw, info.width, info.height, fade);
    await sharp(raw, { raw: { width: info.width, height: info.height, channels: 4 } }).png().toFile(output);
  }
  console.log(`картинка ${info.width}×${info.height}`);
}

async function main() {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      frames: { type: "string" },
      height: { type: "string" },
      columns: { type: "string" },
      range: { type: "string" },
      key: { type: "string", default: "green" },
      "fade-ends": { type: "string" },
    },
  });
  if (positionals.length !== 2) throw new Error(USAGE);
  const [input, output] = positionals;
  const height = positiveInt("height", values.height);
  if (!Object.hasOwn(KEYS, values.key)) throw new Error(`--key должно быть green или magenta, а не «${values.key}»`);
  if (!VIDEO_EXTENSIONS.has(extname(input).toLowerCase())) {
    if ([values.frames, values.columns, values.range].some((value) => value !== undefined)) throw new Error("--frames, --columns и --range — только для видео");
    await makeCutout(input, output, height, values.key, positiveInt("fade-ends", values["fade-ends"]));
    return;
  }
  if (values["fade-ends"] !== undefined) throw new Error("--fade-ends — только для картинки");
  await makeSheet(input, output, {
    height,
    key: values.key,
    frames: positiveInt("frames", values.frames),
    columns: positiveInt("columns", values.columns) ?? 8,
    range: values.range === undefined ? undefined : parseRange(values.range),
  });
}

// Тест импортирует чистые функции этого файла, и импорт не должен запускать нарезку.
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
