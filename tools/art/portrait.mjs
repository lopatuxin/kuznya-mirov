// Портрет героя для экрана героя «Распутья» (холст 900 × 1016 — по четыре на лист атласа движка): ролик покоя (MP4 двойной высоты: сверху цвет, снизу маска) или
// картинка PNG с прозрачностью переносится на холст одного размера для всех героев — середина фигуры и ступни
// на одних местах, рост фигуры задан в точках холста. Так одна панель экрана показывает любого выбранного героя.
// Середина, ступни и макушка фигуры ищутся по маске: непрозрачные точки — фигура, полупрозрачные — тень.
// Выход — всегда ролик MP4 двойной высоты.
// Запуск: node tools/art/portrait.mjs <ролик.mp4 | картинка.png> <выход.mp4> <рост фигуры в точках холста>

import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";
import ffmpegPath from "ffmpeg-static";
import sharp from "sharp";

const CANVAS = [900, 1016];
const CENTER_X = 540;
const FEET_Y = 790;
// Маска фигуры плотнее этого; тени героев не плотнее 203.
const SOLID = 245;

function ffmpeg(args) {
  const run = spawnSync(ffmpegPath, ["-hide_banner", "-loglevel", "error", "-y", ...args], { maxBuffer: 1 << 30 });
  if (run.status !== 0) throw new Error(run.stderr.toString());
  return run.stdout;
}

/** Первый кадр как RGBA: у ролика маска берётся из нижней половины. */
async function firstFrame(input) {
  if (extname(input) === ".png") return sharp(input).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  const png = ffmpeg(["-i", input, "-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"]);
  const { width, height } = await sharp(png).metadata();
  const half = height / 2;
  const color = await sharp(png).extract({ left: 0, top: 0, width, height: half }).removeAlpha().raw().toBuffer();
  const mask = await sharp(png).extract({ left: 0, top: half, width, height: half }).greyscale().raw().toBuffer();
  const data = Buffer.alloc(width * half * 4);
  for (let i = 0; i < width * half; i++) {
    color.copy(data, i * 4, i * 3, i * 3 + 3);
    data[i * 4 + 3] = mask[i];
  }
  return { data, info: { width, height: half } };
}

/** Макушка, ступни и середина фигуры по непрозрачным точкам. */
function measure({ data, info }) {
  let top = -1, feet = -1, sumX = 0, count = 0;
  for (let y = 0; y < info.height; y++) {
    let row = 0;
    for (let x = 0; x < info.width; x++) {
      if (data[(y * info.width + x) * 4 + 3] > SOLID) { row++; sumX += x; count++; }
    }
    if (row > 3) { if (top < 0) top = y; feet = y; }
  }
  if (count === 0) throw new Error("фигура не найдена: в маске нет непрозрачных точек");
  return { top, feet, cx: sumX / count };
}

const [input, output, heightArg] = process.argv.slice(2);
const frame = await firstFrame(input);
const { top, feet, cx } = measure(frame);
const scale = Number(heightArg) / (feet - top);
const w = Math.round((frame.info.width * scale) / 2) * 2;
const h = Math.round((frame.info.height * scale) / 2) * 2;
const x = Math.round(CENTER_X - cx * scale);
const y = Math.round(FEET_Y - feet * scale);

// Портреты одного вида: неподвижный портрет тоже становится роликом —
// кадр двойной высоты, сверху цвет, снизу маска, две секунды одного кадра.
let source = ["-i", input];
if (extname(input) === ".png") {
  const { width, height } = frame.info;
  const stacked = Buffer.alloc(width * height * 2 * 3);
  for (let i = 0; i < width * height; i++) {
    frame.data.copy(stacked, i * 3, i * 4, i * 4 + 3);
    stacked.fill(frame.data[i * 4 + 3], (width * height + i) * 3, (width * height + i) * 3 + 3);
  }
  const still = join(tmpdir(), "portrait-still.png");
  await sharp(stacked, { raw: { width, height: height * 2, channels: 3 } }).png().toFile(still);
  source = ["-loop", "1", "-t", "2", "-r", "24", "-i", still];
}
const half = frame.info.height;
const [cw, ch] = CANVAS;
const graph = [
  `[0:v]split=2[a][b]`,
  `[a]crop=iw:${half}:0:0,scale=${w}:${h}:in_color_matrix=bt709:out_color_matrix=bt709[c]`,
  `[b]crop=iw:${half}:0:${half},scale=${w}:${h}[m]`,
  `color=c=black:s=${cw}x${ch}:r=24,format=yuv420p[bc]`,
  `color=c=black:s=${cw}x${ch}:r=24,format=yuv420p[bm]`,
  `[bc][c]overlay=x=${x}:y=${y}:shortest=1:format=yuv420[oc]`,
  `[bm][m]overlay=x=${x}:y=${y}:shortest=1:format=yuv420[om]`,
  `[oc][om]vstack,format=yuv420p[v]`,
].join(";");
ffmpeg([...source, "-filter_complex", graph, "-map", "[v]", "-an", "-c:v", "libx264", "-crf", "18", "-color_primaries", "bt709", "-color_trc", "iec61966-2-1", "-colorspace", "bt709", "-color_range", "tv", "-preset", "slow", "-movflags", "+faststart", output]);
console.log(`${output}: фигура ${feet - top} → ${heightArg} точек, масштаб ${scale.toFixed(3)}`);
