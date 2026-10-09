// Превращает видео или картинку нейросети на однотонном ярко-зелёном фоне в PNG с прозрачным
// фоном для игры. Траву, листву и всё зелёное нейросеть рисует на ярко-розовом фоне
// (`--key magenta`): зелёный фон убрал бы их вместе с собой. Картинка вырезается по своему содержимому;
// у слоя фона, который повторяется по ширине, `--repeat-x` сводит концы без шва. `--fade-ends` и
// `--repeat-x` считаются в точках картинки после `--height`. Видео становится листом кадров
// сеткой: повторяющийся шаг находится сам, а движение без повтора (удар, падение) задаётся
// номерами кадров через --range. Персонаж в видео должен двигаться на месте при неподвижной
// камере: все кадры режутся одним общим прямоугольником, и если персонаж идёт через кадр, он
// будет перемещаться внутри листа. Видео с выходом `.mp4` становится видео игры двойной высоты:
// сверху цвет, снизу маска прозрачности; петля без скачка находится сама или задаётся --range.
// Зависимости ставятся один раз: `npm install` в `tools/art/`.

import { mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import ffmpegPath from "ffmpeg-static";
import sharp from "sharp";

const VIDEO_EXTENSIONS = new Set([".mp4", ".webm", ".mov"]);
const USAGE = "node tools/art/sheet.mjs <вход.mp4|.png> <выход.png|.mp4> [--frames N] [--height H] [--columns C] [--range A-B] [--key green|magenta] [--fade-ends P] [--repeat-x P]";
const MARGIN = 4;
export const VISIBLE_ALPHA = 26;
const THUMB_SIZE = 96;
const MIN_PERIOD = 4;
const MIN_LOOP = 36;
const LUMA_EDGE = 0.1;
// Тёмная трава на розовом отличается от фона по яркости на ~70 уровней, светлый куст — на 0–8.
const MIN_LUMA_GAP = 32;
const BLEED = 8;

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

/** Уровень, ниже которого лежит половина точек гистограммы; `undefined`, если она пуста. */
function histogramMedian(histogram) {
  const total = histogram.reduce((sum, count) => sum + count, 0);
  if (total === 0) return undefined;
  let seen = 0;
  for (let level = 0; level < histogram.length; level++) {
    seen += histogram[level];
    if (seen * 2 >= total) return level;
  }
  return histogram.length - 1;
}

/**
 * Добавляет к прозрачности кадра видео ключ по яркости. Видео хранит цвет вдвое грубее яркости, и
 * тонкий тёмный стебель на розовом фоне по цвету наполовину розовый, а по яркости — трава. Уровни
 * берутся из самого кадра: фон — медиана яркости точек, которые ключ по цвету счёл чистым фоном,
 * трава — медиана сплошных. Между ними прозрачность растёт по прямой; на десятой доле промежутка от
 * каждого края — уже 0 и 1. Ключ только добавляет непрозрачность: итог — большее из двух ключей. Если яркости
 * фона и предмета ближе `MIN_LUMA_GAP`, ключа по яркости нет: светлый куст на розовом по яркости почти равен фону
 * (разница 0–8 при ряби фона 1–2), и ключ делал бы видимой рябь фона по всему кадру.
 */
export function addLumaKey(rgba, luma) {
  const background = new Array(256).fill(0);
  const solid = new Array(256).fill(0);
  for (let i = 0; i < luma.length; i++) {
    const alpha = rgba[i * 4 + 3];
    if (alpha === 0) background[luma[i]]++;
    else if (alpha === 255) solid[luma[i]]++;
  }
  const backgroundLevel = histogramMedian(background);
  const solidLevel = histogramMedian(solid);
  if (backgroundLevel === undefined || solidLevel === undefined) return;
  const span = backgroundLevel - solidLevel;
  if (Math.abs(span) < MIN_LUMA_GAP) return;
  for (let i = 0; i < luma.length; i++) {
    const t = (backgroundLevel - luma[i]) / span;
    const alpha = Math.round(255 * Math.min(1, Math.max(0, (t - LUMA_EDGE) / (1 - 2 * LUMA_EDGE))));
    if (alpha > rgba[i * 4 + 3]) rgba[i * 4 + 3] = alpha;
  }
}

const lumaOf = (red, green, blue) => 0.299 * red + 0.587 * green + 0.114 * blue;
const chromaBlue = (red, green, blue) => -0.168736 * red - 0.331264 * green + 0.5 * blue;
const chromaRed = (red, green, blue) => 0.5 * red - 0.418688 * green - 0.081312 * blue;

/**
 * Уровни кадра по ключу по цвету: цвет фона — медиана каждого канала по точкам чистого фона, яркость травы — медиана
 * яркости сплошных точек. `undefined`, если чистого фона или сплошных точек нет.
 */
function frameLevels(rgb, rgba) {
  const background = [0, 1, 2].map(() => new Array(256).fill(0));
  const solid = new Array(256).fill(0);
  for (let i = 0; i < rgb.length / 3; i++) {
    const alpha = rgba[i * 4 + 3];
    if (alpha === 0) {
      for (let c = 0; c < 3; c++) background[c][rgb[i * 3 + c]]++;
    } else if (alpha === 255) {
      solid[Math.round(lumaOf(rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]))]++;
    }
  }
  const color = background.map(histogramMedian);
  const solidLuma = histogramMedian(solid);
  return color[0] === undefined || solidLuma === undefined ? undefined : { background: color, solidLuma };
}

/**
 * Цвет кадра видео на розовом фоне без розовой примеси. Видео хранит цвет вдвое грубее яркости, и розовый фона
 * подмешан к цвету тонких колосков и краёв травинок; ключ по цвету снимает только избыток меньшего из красного и
 * синего над зелёным, и оливковый колосок с такой примесью оставался красноватым. Здесь из цветности каждой точки
 * (Cb, Cr) убирается её составляющая вдоль цветности фона `background`. Зелёный, оливковый и золотистый лежат против
 * розового — их собственный оттенок не трогается; красный и синий предмет потерял бы свою розовую часть, поэтому их
 * снимают не на розовом фоне. Яркость сплошной точки остаётся, а у полупрозрачной точки края из яркости вычитается
 * доля фона по её прозрачности: иначе край был бы светлой смесью травы с розовым и светился бы на тёмном. Прозрачность
 * у самого края ключ по яркости нарочно занижает, и вычитание по ней ушло бы в чёрное, поэтому край не темнее
 * сплошной травы `solidLuma`, если сама точка не темнее её. Цвет пишется в `rgba` поверх цвета ключа.
 */
function removeBackgroundChroma(rgb, rgba, { background, solidLuma }) {
  const towardBlue = chromaBlue(...background);
  const towardRed = chromaRed(...background);
  const length = Math.hypot(towardBlue, towardRed);
  const backgroundLuma = lumaOf(...background);
  if (length === 0) return;
  for (let src = 0, dst = 0; src < rgb.length; src += 3, dst += 4) {
    const red = rgb[src];
    const green = rgb[src + 1];
    const blue = rgb[src + 2];
    const alpha = rgba[dst + 3] / 255;
    const mixed = lumaOf(red, green, blue);
    const unmixed = alpha > 0 ? (mixed - (1 - alpha) * backgroundLuma) / alpha : mixed;
    const luma = Math.max(unmixed, Math.min(mixed, solidLuma));
    let cb = chromaBlue(red, green, blue);
    let cr = chromaRed(red, green, blue);
    const along = (cb * towardBlue + cr * towardRed) / length;
    if (along > 0) {
      cb -= (along * towardBlue) / length;
      cr -= (along * towardRed) / length;
    }
    rgba[dst] = Math.round(Math.min(255, Math.max(0, luma + 1.402 * cr)));
    rgba[dst + 1] = Math.round(Math.min(255, Math.max(0, luma - 0.344136 * cb - 0.714136 * cr)));
    rgba[dst + 2] = Math.round(Math.min(255, Math.max(0, luma + 1.772 * cb)));
  }
}

/**
 * Кадр видео в RGBA без фона: ключ по цвету, ключ по плоскости яркости `luma` и, на розовом фоне, цвет без розовой
 * примеси.
 */
export function keyVideoFrame(rgb, luma, key) {
  const rgba = KEYS[key](rgb);
  const levels = key === "magenta" ? frameLevels(rgb, rgba) : undefined;
  addLumaKey(rgba, luma);
  if (levels !== undefined) removeBackgroundChroma(rgb, rgba, levels);
  return rgba;
}

export async function removeBackground(file, key) {
  const { data, info } = await sharp(file).removeAlpha().raw().toBuffer({ resolveWithObject: true });
  return { rgba: KEYS[key](data), width: info.width, height: info.height };
}

async function keyFrame(frame, key) {
  const { data, info } = await sharp(frame.color).removeAlpha().raw().toBuffer({ resolveWithObject: true });
  const luma = await sharp(frame.luma).extractChannel(0).raw().toBuffer();
  return { rgba: keyVideoFrame(data, luma, key), width: info.width, height: info.height };
}

/** Крайние видимые точки картинки, или `null`, если видимых нет. */
export function visibleBounds({ rgba, width, height }) {
  let left = width;
  let top = height;
  let right = -1;
  let bottom = -1;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (rgba[(y * width + x) * 4 + 3] < VISIBLE_ALPHA) continue;
      left = Math.min(left, x);
      top = Math.min(top, y);
      right = Math.max(right, x);
      bottom = Math.max(bottom, y);
    }
  }
  return right < 0 ? null : { left, top, right, bottom };
}

function unionBounds(list) {
  const present = list.filter((bounds) => bounds !== null);
  if (present.length === 0) throw new Error("после удаления фона ничего не осталось");
  return {
    left: Math.min(...present.map((bounds) => bounds.left)),
    top: Math.min(...present.map((bounds) => bounds.top)),
    right: Math.max(...present.map((bounds) => bounds.right)),
    bottom: Math.max(...present.map((bounds) => bounds.bottom)),
  };
}

function boxAround(bounds, width, height) {
  const left = Math.max(0, bounds.left - MARGIN);
  const top = Math.max(0, bounds.top - MARGIN);
  const right = Math.min(width - 1, bounds.right + MARGIN);
  const bottom = Math.min(height - 1, bounds.bottom + MARGIN);
  return { left, top, width: right - left + 1, height: bottom - top + 1 };
}

// Один прямоугольник на все кадры: иначе персонаж прыгал бы внутри листа от кадра к кадру.
export function visibleBox(images) {
  return boxAround(unionBounds(images.map(visibleBounds)), images[0].width, images[0].height);
}

function cropped({ rgba, width, height }, box, targetHeight) {
  const image = sharp(rgba, { raw: { width, height, channels: 4 } }).extract(box);
  return targetHeight ? image.resize({ height: targetHeight, kernel: "lanczos3" }) : image;
}

export function cutOut(image, box, targetHeight) {
  return cropped(image, box, targetHeight).png().toBuffer({ resolveWithObject: true });
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

/** Кадры видео: цвет и плоскость яркости в полном разрешении — цвет видео хранит вдвое грубее. */
function extractFrames(input, dir) {
  const run = runFfmpeg([
    "-hide_banner", "-loglevel", "error", "-i", input,
    "-fps_mode", "passthrough", join(dir, "f%06d.png"),
    "-vf", "extractplanes=y", "-fps_mode", "passthrough", join(dir, "y%06d.png"),
  ]);
  if (run.status !== 0) throw new Error(`ffmpeg не разрезал видео: ${lastLine(run.stderr)}`);
  return readdirSync(dir)
    .filter((name) => name.startsWith("f") && name.endsWith(".png"))
    .sort()
    .map((name) => ({ color: join(dir, name), luma: join(dir, `y${name.slice(1)}`) }));
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

/**
 * Петля видео игры по эскизам масок кадров: отрезок не короче `MIN_LOOP` кадров, у которого кадр за
 * последним меньше всего отличается от первого, — с последнего кадра видео переходит на первый как
 * на следующий. Из равных берётся более короткий.
 */
export function findLoop(masks) {
  let best = { start: 0, length: 0, difference: Infinity };
  for (let length = MIN_LOOP; length < masks.length; length++) {
    for (let start = 0; start + length < masks.length; start++) {
      const difference = meanDifference(masks[start], masks[start + length]);
      if (difference < best.difference) best = { start, length, difference };
    }
  }
  if (best.length === 0) throw new Error(`в видео меньше ${MIN_LOOP + 1} кадров, петлю не найти — задай кадры через --range`);
  return best;
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
        sharp(file.color).resize(THUMB_SIZE, THUMB_SIZE, { fit: "fill" }).greyscale().raw().toBuffer()));
      span = findCycle(thumbs);
    }
    const count = Math.min(options.frames ?? span.length, span.length);
    const images = await Promise.all(pickFrames(span, count).map((i) => keyFrame(files[i], options.key)));
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
 * конец одного лежит поверх или под непрозрачной серединой другого. У видео крайние точки `bounds`
 * общие для всех кадров, иначе угасание ходило бы вслед за травинками.
 */
export function fadeEnds(rgba, width, height, fade, bounds = visibleBounds({ rgba, width, height })) {
  if (bounds === null) return;
  const { left, right } = bounds;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const alpha = (y * width + x) * 4 + 3;
      rgba[alpha] = Math.round(rgba[alpha] * smoothstep(0, fade, x - left) * smoothstep(0, fade, right - x));
    }
  }
}

/**
 * Сводит концы картинки, которая повторяется по ширине: правые `overlap` столбцов плавно
 * переходят в левые и ложатся на них, картинка становится на `overlap` уже. Последний столбец
 * результата в исходнике — сосед первого, поэтому повтор идёт без шва. Смешивание — с учётом
 * прозрачности: прозрачная точка не темнит соседнюю видимую. Если край силуэта на концах на
 * разной высоте, в полосе наложения видны оба края, полупрозрачными.
 */
export function wrapEnds(rgba, width, height, overlap) {
  const outWidth = width - overlap;
  const out = Buffer.alloc(outWidth * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < outWidth; x++) {
      const head = (y * width + x) * 4;
      const dst = (y * outWidth + x) * 4;
      if (x >= overlap) {
        rgba.copy(out, dst, head, head + 4);
        continue;
      }
      const tail = (y * width + outWidth + x) * 4;
      const headWeight = smoothstep(0, overlap, x) * rgba[head + 3];
      const tailWeight = (1 - smoothstep(0, overlap, x)) * rgba[tail + 3];
      const alpha = headWeight + tailWeight;
      for (let c = 0; c < 3; c++) out[dst + c] = alpha === 0 ? 0 : Math.round((rgba[head + c] * headWeight + rgba[tail + c] * tailWeight) / alpha);
      out[dst + 3] = Math.round(alpha);
    }
  }
  return out;
}

/**
 * Дополняет кадр прозрачной строкой сверху и столбцом справа до чётных сторон: видео хранит цвет
 * по квадратам 2 × 2 точки, и половины видео должны начинаться с целого квадрата.
 */
export function padToEven(rgba, width, height) {
  const outWidth = width + (width % 2);
  const outHeight = height + (height % 2);
  if (outWidth === width && outHeight === height) return { rgba, width, height };
  const out = Buffer.alloc(outWidth * outHeight * 4);
  const top = outHeight - height;
  for (let y = 0; y < height; y++) rgba.copy(out, ((y + top) * outWidth) * 4, y * width * 4, (y + 1) * width * 4);
  return { rgba: out, width: outWidth, height: outHeight };
}

/**
 * Под прозрачными точками — продолжение цвета ближайших видимых на `BLEED` точек, дальше чёрный.
 * Видео сжимает цвет блоками и вдвое грубее яркости: чёрный или розовый под прозрачной точкой
 * затёк бы на соседний край травы, и он потемнел бы или порозовел. Каждая новая точка берёт средний
 * цвет уже окрашенных соседей, кольцо за кольцом.
 */
export function bleedColor(rgba, width, height) {
  const filled = new Uint8Array(width * height);
  for (let i = 0; i < filled.length; i++) {
    if (rgba[i * 4 + 3] > 0) filled[i] = 1;
    else rgba.fill(0, i * 4, i * 4 + 3);
  }
  const neighbours = (i, visit) => {
    const x = i % width;
    const y = (i - x) / width;
    for (let dy = -1; dy <= 1; dy++) {
      for (let dx = -1; dx <= 1; dx++) {
        const nx = x + dx;
        const ny = y + dy;
        if ((dx !== 0 || dy !== 0) && nx >= 0 && nx < width && ny >= 0 && ny < height) visit(ny * width + nx);
      }
    }
  };
  const queued = new Uint8Array(width * height);
  let ring = [];
  for (let i = 0; i < filled.length; i++) {
    if (filled[i]) continue;
    neighbours(i, (n) => {
      if (filled[n] && !queued[i]) {
        queued[i] = 1;
        ring.push(i);
      }
    });
  }
  for (let step = 0; step < BLEED && ring.length > 0; step++) {
    const colors = ring.map((i) => {
      const sum = [0, 0, 0];
      let count = 0;
      neighbours(i, (n) => {
        if (!filled[n]) return;
        for (let c = 0; c < 3; c++) sum[c] += rgba[n * 4 + c];
        count++;
      });
      return sum.map((value) => Math.round(value / count));
    });
    ring.forEach((i, k) => {
      for (let c = 0; c < 3; c++) rgba[i * 4 + c] = colors[k][c];
      filled[i] = 1;
    });
    const next = [];
    for (const i of ring) {
      neighbours(i, (n) => {
        if (filled[n] || queued[n]) return;
        queued[n] = 1;
        next.push(n);
      });
    }
    ring = next;
  }
}

/** Кадр видео игры в RGB двойной высоты: сверху цвет, снизу прозрачность серым — белое непрозрачно. */
export function stackColorAndMask(rgba, width, height) {
  const out = Buffer.alloc(width * height * 2 * 3);
  const maskStart = width * height * 3;
  for (let i = 0; i < width * height; i++) {
    out[i * 3] = rgba[i * 4];
    out[i * 3 + 1] = rgba[i * 4 + 1];
    out[i * 3 + 2] = rgba[i * 4 + 2];
    out.fill(rgba[i * 4 + 3], maskStart + i * 3, maskStart + i * 3 + 3);
  }
  return out;
}

async function maskThumb({ rgba, width, height }) {
  return sharp(rgba, { raw: { width, height, channels: 4 } })
    .resize(THUMB_SIZE, THUMB_SIZE, { fit: "fill" })
    .extractChannel(3)
    .raw()
    .toBuffer();
}

/**
 * Видео игры из ролика: петля кадров без скачка, фон снят ключами по цвету и яркости, кадр вырезан
 * одним прямоугольником на все кадры. Каждый кадр выхода — цвет над маской; сжатие H.264 с цветом
 * sRGB, чтобы браузер отдал маску теми же числами.
 */
export async function makeVideo(input, output, options) {
  const dir = mkdtempSync(join(tmpdir(), "kuznya-video-"));
  try {
    const fps = readFps(input);
    const frames = extractFrames(input, dir);
    let span = options.range;
    if (span && span.start + span.length > frames.length) throw new Error(`--range выходит за конец видео (кадров в видео: ${frames.length})`);
    const bounds = [];
    const masks = [];
    let source;
    for (const frame of frames) {
      const image = await keyFrame(frame, options.key);
      source = { width: image.width, height: image.height };
      bounds.push(visibleBounds(image));
      if (!span) masks.push(await maskThumb(image));
    }
    let jump;
    if (!span) {
      const loop = findLoop(masks);
      span = { start: loop.start, length: loop.length };
      jump = loop.difference;
    }
    const visible = unionBounds(bounds.slice(span.start, span.start + span.length));
    const box = boxAround(visible, source.width, source.height);
    const scale = options.height ? options.height / box.height : 1;
    let size;
    for (let k = 0; k < span.length; k++) {
      const image = await keyFrame(frames[span.start + k], options.key);
      if (options.fade !== undefined) fadeEnds(image.rgba, image.width, image.height, options.fade / scale, visible);
      const { data, info } = await cropped(image, box, options.height).raw().toBuffer({ resolveWithObject: true });
      const frame = padToEven(data, info.width, info.height);
      bleedColor(frame.rgba, frame.width, frame.height);
      size = { width: frame.width, height: frame.height };
      await sharp(stackColorAndMask(frame.rgba, frame.width, frame.height), { raw: { width: frame.width, height: frame.height * 2, channels: 3 } })
        .png({ compressionLevel: 1 })
        .toFile(join(dir, `v${String(k + 1).padStart(6, "0")}.png`));
    }
    const run = runFfmpeg([
      "-hide_banner", "-loglevel", "error", "-y",
      "-framerate", String(fps), "-i", join(dir, "v%06d.png"),
      "-vf", "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p",
      "-c:v", "libx264", "-preset", "slow", "-crf", "16", "-pix_fmt", "yuv420p",
      "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "iec61966-2-1", "-color_range", "tv",
      "-an", "-movflags", "+faststart", output,
    ]);
    if (run.status !== 0) throw new Error(`ffmpeg не собрал видео: ${lastLine(run.stderr)}`);
    console.log(`кадры видео ${span.start + 1}-${span.start + span.length} из ${frames.length}, длина ${span.length} (${(span.length / fps).toFixed(2)} с)`);
    if (jump !== undefined) console.log(`скачок на стыке петли: ${jump.toFixed(2)} из 255`);
    console.log(`кадр ${size.width}×${size.height}, видео ${size.width}×${size.height * 2}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

async function makeCutout(input, output, height, key, fade, overlap) {
  const image = await removeBackground(input, key);
  const { data, info } = await cutOut(image, visibleBox([image]), height);
  let width = info.width;
  if (fade === undefined && overlap === undefined) {
    writeFileSync(output, data);
  } else {
    let raw = await sharp(data).raw().toBuffer();
    if (fade !== undefined) fadeEnds(raw, width, info.height, fade);
    if (overlap !== undefined) {
      if (overlap * 2 > width) throw new Error(`--repeat-x больше половины ширины картинки (${width})`);
      raw = wrapEnds(raw, width, info.height, overlap);
      width -= overlap;
    }
    await sharp(raw, { raw: { width, height: info.height, channels: 4 } }).png().toFile(output);
  }
  console.log(`картинка ${width}×${info.height}`);
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
      "repeat-x": { type: "string" },
    },
  });
  if (positionals.length !== 2) throw new Error(USAGE);
  const [input, output] = positionals;
  const height = positiveInt("height", values.height);
  if (!Object.hasOwn(KEYS, values.key)) throw new Error(`--key должно быть green или magenta, а не «${values.key}»`);
  const toVideo = extname(output).toLowerCase() === ".mp4";
  if (!VIDEO_EXTENSIONS.has(extname(input).toLowerCase())) {
    if (toVideo) throw new Error("видео игры делается только из видео");
    if ([values.frames, values.columns, values.range].some((value) => value !== undefined)) throw new Error("--frames, --columns и --range — только для видео");
    if (values["fade-ends"] !== undefined && values["repeat-x"] !== undefined) throw new Error("--fade-ends и --repeat-x не бывают вместе: гашёные концы повтор не сведёт");
    await makeCutout(input, output, height, values.key, positiveInt("fade-ends", values["fade-ends"]), positiveInt("repeat-x", values["repeat-x"]));
    return;
  }
  const range = values.range === undefined ? undefined : parseRange(values.range);
  if (toVideo) {
    const sheetOnly = ["frames", "columns", "repeat-x"].find((name) => values[name] !== undefined);
    if (sheetOnly !== undefined) throw new Error(`--${sheetOnly} не бывает у видео игры`);
    await makeVideo(input, output, { height, key: values.key, range, fade: positiveInt("fade-ends", values["fade-ends"]) });
    return;
  }
  const pictureOnly = ["fade-ends", "repeat-x"].find((name) => values[name] !== undefined);
  if (pictureOnly !== undefined) throw new Error(`--${pictureOnly} — только для картинки и видео игры`);
  await makeSheet(input, output, {
    height,
    key: values.key,
    frames: positiveInt("frames", values.frames),
    columns: positiveInt("columns", values.columns) ?? 8,
    range,
  });
}

// Тест импортирует чистые функции этого файла, и импорт не должен запускать нарезку.
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
