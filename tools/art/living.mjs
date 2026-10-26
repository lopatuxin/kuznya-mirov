// Живой фон из ролика нейросети — вся картина начального экрана «Распутья» роликом по кругу. Места, которые нейросеть
// не оживила, а перерисовала (тёмный лес), и места, где на картине есть то, чего в ролике нет (строки, высеченные на
// камне), остаются неподвижными — из `still`: первого кадра того же ролика, где строки высечены, поэтому на мягком
// краю этих мест цвет ролика встречается с тем же цветом. Нейросеть к началу ролика не возвращается, и любая склейка
// конца с началом видна, поэтому круг идёт туда и обратно: листва, трава и туман назад качаются так же естественно.
// Описание — JSON (места в точках картины игры):
//   { "video": путь к ролику, "rect": [x, y, w, h] — где на картине лежит кадр ролика целиком,
//     "hold": [[[x, y], ...], ...] — неподвижные места, "feather": радиус размытия их края и края ролика,
//     "still": неподвижный кадр размера картины игры, "picture": картина игры под первым кадром для `--first` }
// Пишет видео игры размера всей картины: сверху цвет, снизу маска. Вне `rect` — неподвижный кадр, и у края
// `rect`, не совпавшего с краем картины, ролик мягко переходит в него: шва нет в любом окне.
// Запуск: node tools/art/living.mjs <описание.json> <выход.mp4>
// Первый кадр на картине для строк камня и реки: node tools/art/living.mjs <описание.json> --first <кадр.png>

import { mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import sharp from "sharp";
import { encodeVideo, readFps, runFfmpeg, stackColorAndMask } from "./sheet.mjs";
import { inside } from "./flow.mjs";

const USAGE = "node tools/art/living.mjs <описание.json> <выход.mp4> | <описание.json> --first <кадр.png>";

/** Кадр ролика для кадра круга `k`: вперёд, потом назад без крайних кадров; длина круга — `2·count − 2`. */
export function pingpongFrame(k, count) {
  return k < count ? k : 2 * count - 2 - k;
}

function blur(values, width, height, radius) {
  const out = Float32Array.from(values);
  const tmp = new Float32Array(values.length);
  const r = Math.max(1, Math.round(radius));
  for (let pass = 0; pass < 3; pass++) {
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        let sum = 0;
        for (let k = -r; k <= r; k++) sum += out[y * width + Math.min(width - 1, Math.max(0, x + k))];
        tmp[y * width + x] = sum / (2 * r + 1);
      }
    }
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        let sum = 0;
        for (let k = -r; k <= r; k++) sum += tmp[Math.min(height - 1, Math.max(0, y + k)) * width + x];
        out[y * width + x] = sum / (2 * r + 1);
      }
    }
  }
  return out;
}

/**
 * Доля 0..1 ролика в каждой точке картины `width` × `height`: 1 — ролик, 0 — неподвижное место; ролик лежит в
 * `rect`, а у его края и у мест `hold` переходит в неподвижный кадр мягко — размытием радиуса `feather`.
 */
function movingShare(piece, width, height) {
  const [left, top, rectWidth, rectHeight] = piece.rect;
  // Три прохода размытия дотягиваются на три радиуса: на столько доля отступает от края ролика и у самого края
  // сходит ровно к нулю, а не обрывается.
  const inset = 3 * piece.feather;
  // Край ролика, совпавший с краем картины, не отступает: за ним ничего нет.
  const within = (at, start, size, total) =>
    at >= start && at < start + size && (start === 0 || at - start >= inset) && (start + size === total || start + size - at > inset);
  const shown = new Float32Array(width * height);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const held = piece.hold.some((polygon) => inside(polygon, x + 0.5, y + 0.5));
      shown[y * width + x] = within(x, left, rectWidth, width) && within(y, top, rectHeight, height) && !held ? 1 : 0;
    }
  }
  const share = blur(shown, width, height, piece.feather);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (x < left || x >= left + rectWidth || y < top || y >= top + rectHeight) share[y * width + x] = 0;
    }
  }
  return share;
}

async function bake(descriptionFile, output) {
  const piece = JSON.parse(readFileSync(descriptionFile, "utf8"));
  const [left, top, rectWidth, rectHeight] = piece.rect;
  const still = await sharp(piece.still).removeAlpha().raw().toBuffer({ resolveWithObject: true });
  const { width, height } = still.info;
  if (width % 2 || height % 2) throw new Error(`у неподвижного кадра должны быть чётные стороны, а не ${width}×${height}`);
  const dir = mkdtempSync(join(tmpdir(), "kuznya-living-"));
  try {
    const fps = readFps(piece.video);
    const cut = runFfmpeg([
      "-hide_banner", "-loglevel", "error", "-i", piece.video, "-vf", `scale=${rectWidth}:${rectHeight}:flags=lanczos`,
      join(dir, "in%06d.png"),
    ]);
    if (cut.status !== 0) throw new Error(`ffmpeg не разрезал ролик: ${cut.stderr.trim().split("\n").pop()}`);
    const names = readdirSync(dir).filter((name) => name.startsWith("in")).sort();
    const frames = [];
    for (const name of names) frames.push(await sharp(join(dir, name)).removeAlpha().raw().toBuffer());
    const share = movingShare(piece, width, height);
    const total = width * height;
    const count = 2 * frames.length - 2;
    for (let k = 0; k < count; k++) {
      const frame = frames[pingpongFrame(k, frames.length)];
      const rgba = Buffer.alloc(total * 4, 255);
      for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
          const i = y * width + x;
          const a = share[i];
          const j = ((y - top) * rectWidth + (x - left)) * 3;
          for (let c = 0; c < 3; c++) {
            rgba[i * 4 + c] = Math.round(a > 0 ? frame[j + c] * a + still.data[i * 3 + c] * (1 - a) : still.data[i * 3 + c]);
          }
        }
      }
      await sharp(stackColorAndMask(rgba, width, height), { raw: { width, height: height * 2, channels: 3 } })
        .png({ compressionLevel: 1 })
        .toFile(join(dir, `v${String(k + 1).padStart(6, "0")}.png`));
    }
    encodeVideo(join(dir, "v%06d.png"), output, fps);
    console.log(`фон ${width}×${height}, кадров ${count}, ${(count / fps).toFixed(2)} с по кругу`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/**
 * Первый кадр ролика той же нарезкой, что у живого фона, положенный на картину `picture` по `rect`: из него
 * высекаются строки для `still` и делается кусок реки — чтобы они легли на ролик точка в точку.
 */
async function firstFrame(descriptionFile, output) {
  const piece = JSON.parse(readFileSync(descriptionFile, "utf8"));
  const [left, top, rectWidth, rectHeight] = piece.rect;
  const dir = mkdtempSync(join(tmpdir(), "kuznya-living-"));
  try {
    const cut = runFfmpeg([
      "-hide_banner", "-loglevel", "error", "-i", piece.video, "-vf", `scale=${rectWidth}:${rectHeight}:flags=lanczos`,
      "-frames:v", "1", join(dir, "first.png"),
    ]);
    if (cut.status !== 0) throw new Error(`ffmpeg не достал первый кадр: ${cut.stderr.trim().split("\n").pop()}`);
    await sharp(piece.picture).composite([{ input: join(dir, "first.png"), left, top }]).png().toFile(output);
    console.log(`первый кадр на картине — ${output}`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

async function main() {
  const args = process.argv.slice(2);
  if (args.length === 3 && args[1] === "--first") {
    await firstFrame(args[0], args[2]);
    return;
  }
  if (args.length !== 2 || !/\.mp4$/i.test(args[1])) throw new Error(USAGE);
  await bake(args[0], args[1]);
}

// Тест импортирует чистые функции этого файла, и импорт не должен запускать сборку видео.
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
