// Живой кусок воды из картины — река начального экрана «Распутья» поверх живого фона, который идёт туда и обратно:
// вода в куске течёт по руслу всегда вниз, пена бурлит, над водой тянется лёгкий туман; всё остальное остаётся
// картиной точка в точку. Течение — два слоя картины, сдвинутых по течению со сдвигом полпериода и смешанных
// треугольником, как карта течения в играх: каждый слой уходит по течению и сменяется другим, пока сдвинут дальше
// всего, поэтому видео идёт по кругу без рывка.
// Вода — точки внутри многоугольника, синие или белые от пены; камни, деревья и берега не двигаются.
// Описание куска — JSON (места в точках картины):
//   { "rect": [x, y, w, h], "polygon": [[x, y], ...], "flow": [[x, y, dx, dy], ...], "speed": точек в секунду,
//     "period": секунд, "mist": [[x, y, радиус], ...] }
// `flow` — направления течения в нескольких точках, между ними направление смешивается по близости.
// Пишет видео игры: сверху цвет, снизу маска; кусок стоит на картине по `rect`.
// Запуск: node tools/art/flow.mjs <картина.png> <кусок.json> <выход.mp4> [--preview <просмотр.mp4>]

import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import sharp from "sharp";
import { bleedColor, encodeVideo, runFfmpeg, smoothstep, stackColorAndMask } from "./sheet.mjs";

const USAGE = "node tools/art/flow.mjs <картина.png> <кусок.json> <выход.mp4> [--preview <просмотр.mp4>]";
const FPS = 24;
// Насколько синей или белой должна быть точка, чтобы считаться водой: синий выше красного на столько, и яркость.
const BLUE_OVER_RED = 38;
const BLUE_MIN = 150;
const FOAM_MIN = 195;
// Размытие края маски воды в точках картины: вода растворяется в берегу.
const EDGE_BLUR = 1;
// Пена: насколько светлеет белая вода в гребнях шума; размер пятен шума в точках картины.
const FOAM_GAIN = 40;
const FOAM_SCALE = 3.5;
// Туман: плотность, размер клубов в точках картины, скорость сноса вправо в точках в секунду.
const MIST_DENSITY = 0.14;
const MIST_SCALE = 14;
const MIST_DRIFT = 4;
const MIST_COLOR = [0xf2, 0xf6, 0xff];

/** Доля 0..1 воды в точке цвета `[r, g, b]`: синяя вода или белая пена. */
export function waterness(r, g, b) {
  const blue = smoothstep(BLUE_OVER_RED - 10, BLUE_OVER_RED + 10, b - r) * smoothstep(BLUE_MIN - 20, BLUE_MIN + 20, b);
  const foam = smoothstep(FOAM_MIN - 15, FOAM_MIN + 15, Math.min(r, g, b)) * smoothstep(-25, 0, b - r);
  return Math.max(blue, foam);
}

/** Веса двух слоёв течения в доле периода `phase`: каждый слой исчезает, пока сдвинут дальше всего. */
export function layerWeights(phase) {
  const first = (phase % 1 + 1) % 1;
  const w = 1 - Math.abs(1 - 2 * first);
  return [w, 1 - w];
}

export function inside(polygon, x, y) {
  let hit = false;
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const [xi, yi] = polygon[i];
    const [xj, yj] = polygon[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) hit = !hit;
  }
  return hit;
}

/** Направление течения в точке: смесь заданных направлений с весом по обратному квадрату расстояния. */
function flowAt(flow, x, y) {
  let dx = 0;
  let dy = 0;
  let total = 0;
  for (const [fx, fy, fdx, fdy] of flow) {
    const w = 1 / ((x - fx) ** 2 + (y - fy) ** 2 + 1);
    const length = Math.hypot(fdx, fdy) || 1;
    dx += (w * fdx) / length;
    dy += (w * fdy) / length;
    total += w;
  }
  const length = Math.hypot(dx, dy) || 1;
  return [dx / length, dy / length];
}

function hash(x, y) {
  let h = Math.imul(x, 374761393) + Math.imul(y, 668265263);
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

function noise(x, y) {
  const x0 = Math.floor(x);
  const y0 = Math.floor(y);
  const fx = x - x0;
  const fy = y - y0;
  const sx = fx * fx * (3 - 2 * fx);
  const sy = fy * fy * (3 - 2 * fy);
  const top = hash(x0, y0) + (hash(x0 + 1, y0) - hash(x0, y0)) * sx;
  const bottom = hash(x0, y0 + 1) + (hash(x0 + 1, y0 + 1) - hash(x0, y0 + 1)) * sx;
  return top + (bottom - top) * sy;
}

/** Цвет картины в дробной точке, двойной линейной выборкой. */
function sample(rgb, width, height, x, y, out) {
  const cx = Math.min(width - 1.001, Math.max(0, x));
  const cy = Math.min(height - 1.001, Math.max(0, y));
  const x0 = Math.floor(cx);
  const y0 = Math.floor(cy);
  const fx = cx - x0;
  const fy = cy - y0;
  for (let c = 0; c < 3; c++) {
    const a = rgb[(y0 * width + x0) * 3 + c];
    const b = rgb[(y0 * width + x0 + 1) * 3 + c];
    const d = rgb[((y0 + 1) * width + x0) * 3 + c];
    const e = rgb[((y0 + 1) * width + x0 + 1) * 3 + c];
    out[c] = (a + (b - a) * fx) * (1 - fy) + (d + (e - d) * fx) * fy;
  }
  return out;
}

function waterAt(water, width, height, x, y) {
  const cx = Math.min(width - 1, Math.max(0, Math.round(x)));
  const cy = Math.min(height - 1, Math.max(0, Math.round(y)));
  return water[cy * width + cx];
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

/** Кадры куска: RGBA каждого кадра и неподвижный кусок картины под ним. */
async function render(picture, piece) {
  const [left, top, width, height] = piece.rect;
  const rgb = await sharp(picture).removeAlpha().extract({ left, top, width, height }).raw().toBuffer();
  const total = width * height;
  const water = new Float32Array(total);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = y * width + x;
      if (!inside(piece.polygon, left + x + 0.5, top + y + 0.5)) continue;
      water[i] = waterness(rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]);
    }
  }
  const edge = blur(water, width, height, EDGE_BLUR);
  const directions = new Float32Array(total * 2);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const [dx, dy] = flowAt(piece.flow, left + x, top + y);
      directions[(y * width + x) * 2] = dx;
      directions[(y * width + x) * 2 + 1] = dy;
    }
  }
  // Сдвиг одного слоя за полный его цикл — путь воды за период.
  const reach = piece.speed * piece.period;
  const count = Math.round(piece.period * FPS);
  const mist = piece.mist ?? [];
  const frames = [];
  const a = [0, 0, 0];
  const b = [0, 0, 0];
  for (let f = 0; f < count; f++) {
    const time = f / FPS;
    const rgba = Buffer.alloc(total * 4);
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const i = y * width + x;
        const dx = directions[i * 2];
        const dy = directions[i * 2 + 1];
        let color = [rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]];
        let alpha = 0;
        if (edge[i] > 0.01) {
          // Своя доля периода у каждого места воды по медленному шуму: слои не мигают разом по всему руслу.
          const offset = noise((left + x) / 20, (top + y) / 20) * 0.6;
          const phase = time / piece.period + offset;
          const [w0, w1] = layerWeights(phase);
          const p0 = ((phase % 1) + 1) % 1;
          const p1 = (p0 + 0.5) % 1;
          sample(rgb, width, height, x - dx * reach * p0, y - dy * reach * p0, a);
          sample(rgb, width, height, x - dx * reach * p1, y - dy * reach * p1, b);
          // Слой, принёсший в воду камень или берег, уступает место картине: течение несёт только воду.
          const u0 = w0 * waterAt(water, width, height, x - dx * reach * p0, y - dy * reach * p0);
          const u1 = w1 * waterAt(water, width, height, x - dx * reach * p1, y - dy * reach * p1);
          const moved = [0, 1, 2].map((c) => a[c] * u0 + b[c] * u1 + rgb[i * 3 + c] * (1 - u0 - u1));
          // Пена: шум уносится течением теми же двумя слоями и светлит белую воду.
          const n0 = noise((x - dx * reach * p0) / FOAM_SCALE + 31, (y - dy * reach * p0) / FOAM_SCALE);
          const n1 = noise((x - dx * reach * p1) / FOAM_SCALE + 57, (y - dy * reach * p1) / FOAM_SCALE);
          const crest = smoothstep(0.55, 0.9, n0 * w0 + n1 * w1);
          const white = smoothstep(120, 220, Math.min(moved[0], moved[1], moved[2]));
          const lift = FOAM_GAIN * crest * (0.35 + 0.65 * white);
          color = moved.map((value, c) => Math.min(255, value + lift * (c === 2 ? 0.8 : 1)));
          alpha = edge[i];
        }
        // Туман над водой: клубы сносит вправо, по кругу за период.
        let haze = 0;
        for (const [mx, my, radius] of mist) {
          const d = Math.hypot(left + x - mx, top + y - my) / radius;
          if (d < 1) haze = Math.max(haze, 1 - smoothstep(0.3, 1, d));
        }
        if (haze > 0) {
          const drift = MIST_DRIFT * piece.period;
          const q0 = (time / piece.period) % 1;
          const q1 = (q0 + 0.5) % 1;
          const [v0, v1] = layerWeights(time / piece.period);
          const m = noise((x - drift * q0) / MIST_SCALE + 3, y / MIST_SCALE) * v0 + noise((x - drift * q1) / MIST_SCALE + 91, y / MIST_SCALE) * v1;
          const cover = MIST_DENSITY * haze * smoothstep(0.35, 0.8, m);
          if (cover > 0) {
            const under = alpha;
            const out = cover + under * (1 - cover);
            color = color.map((value, c) => (MIST_COLOR[c] * cover + value * under * (1 - cover)) / out);
            alpha = out;
          }
        }
        for (let c = 0; c < 3; c++) rgba[i * 4 + c] = Math.round(color[c]);
        rgba[i * 4 + 3] = Math.round(255 * Math.min(1, alpha));
      }
    }
    frames.push(rgba);
  }
  return { frames, width, height, still: rgb };
}

// Видео требует чётных сторон: кусок ужимается на точку, а не растёт за край картины.
const even = (value) => value - (value % 2);

async function bake(pictureFile, pieceFile, output, preview) {
  const piece = JSON.parse(readFileSync(pieceFile, "utf8"));
  piece.rect = [piece.rect[0], piece.rect[1], even(piece.rect[2]), even(piece.rect[3])];
  // Круг — целое число кадров: иначе последний кадр не сходится с первым.
  piece.period = Math.max(1, Math.round(piece.period * FPS)) / FPS;
  const { frames, width, height, still } = await render(pictureFile, piece);
  const dir = mkdtempSync(join(tmpdir(), "kuznya-water-"));
  try {
    for (let k = 0; k < frames.length; k++) {
      const name = String(k + 1).padStart(6, "0");
      if (preview) {
        const over = Buffer.alloc(width * height * 3);
        for (let i = 0; i < width * height; i++) {
          const a = frames[k][i * 4 + 3] / 255;
          for (let c = 0; c < 3; c++) over[i * 3 + c] = Math.round(frames[k][i * 4 + c] * a + still[i * 3 + c] * (1 - a));
        }
        await sharp(over, { raw: { width, height, channels: 3 } }).png({ compressionLevel: 1 }).toFile(join(dir, `p${name}.png`));
      }
      bleedColor(frames[k], width, height);
      await sharp(stackColorAndMask(frames[k], width, height), { raw: { width, height: height * 2, channels: 3 } })
        .png({ compressionLevel: 1 })
        .toFile(join(dir, `v${name}.png`));
    }
    encodeVideo(join(dir, "v%06d.png"), output, FPS);
    if (preview) {
      // Просмотр: кусок поверх картины, крупно и три круга подряд — видно, есть ли рывок на стыке.
      const run = runFfmpeg([
        "-hide_banner", "-loglevel", "error", "-y", "-stream_loop", "2", "-framerate", String(FPS), "-i", join(dir, "p%06d.png"),
        "-vf", `scale=${width * 3}:${height * 3}:flags=lanczos,format=yuv420p`, "-c:v", "libx264", "-crf", "16", preview,
      ]);
      if (run.status !== 0) throw new Error(`ffmpeg не собрал просмотр: ${run.stderr.trim().split("\n").pop()}`);
    }
    console.log(`кусок ${width}×${height} с (${piece.rect[0]}, ${piece.rect[1]}), кадров ${frames.length}, ${piece.period.toFixed(2)} с по кругу`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

async function main() {
  const { values, positionals } = parseArgs({ allowPositionals: true, options: { preview: { type: "string" } } });
  if (positionals.length !== 3 || !/\.mp4$/i.test(positionals[2])) throw new Error(USAGE);
  await bake(positionals[0], positionals[1], positionals[2], values.preview);
}

// Тест импортирует чистые функции этого файла, и импорт не должен запускать сборку видео.
if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
