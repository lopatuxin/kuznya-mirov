import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Демо-картинки синтезируются кодом, а не скачиваются: так у них нет лицензии, которую надо
// помнить, и их можно пересобрать другого размера или цвета правкой одного числа. Разбора PNG
// в проекте нигде нет — только эта запись, вручную, через встроенный в Node `zlib`.
// Запуск: `node scripts/generateDemoImages.mjs` из `web/`. Результат детерминирован.

const CELL = 32;

const scriptDir = fileURLToPath(new URL(".", import.meta.url));
const gamesDir = resolve(scriptDir, "../../games");

const CRC_TABLE = buildCrcTable();

function buildCrcTable() {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[n] = c >>> 0;
  }
  return table;
}

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const typeBytes = Buffer.from(type, "ascii");
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBytes, data])), 0);
  return Buffer.concat([length, typeBytes, data, crc]);
}

const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

// Цвет, кадры и прозрачность — всё, что видит игра («Картинки»); фильтр построчный (0 — «без
// фильтра») и глубина 8 бит на канал RGBA — самый простой валидный PNG, сжатие берёт на себя
// один вызов `deflateSync`.
function encodePng(width, height, rgba) {
  const ihdrData = Buffer.alloc(13);
  ihdrData.writeUInt32BE(width, 0);
  ihdrData.writeUInt32BE(height, 4);
  ihdrData[8] = 8; // глубина канала
  ihdrData[9] = 6; // цветовой тип: RGBA
  ihdrData[10] = 0; // сжатие: deflate (единственное, что знает формат)
  ihdrData[11] = 0; // фильтр: адаптивный набор фильтров не используется
  ihdrData[12] = 0; // чересстрочность выключена

  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    const rowStart = y * (stride + 1);
    raw[rowStart] = 0; // байт фильтра строки: «без фильтра»
    rgba.copy(raw, rowStart + 1, y * stride, (y + 1) * stride);
  }

  return Buffer.concat([
    PNG_SIGNATURE,
    pngChunk("IHDR", ihdrData),
    pngChunk("IDAT", deflateSync(raw)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

function hexToRgb(hex) {
  const value = hex.replace("#", "");
  return [parseInt(value.slice(0, 2), 16), parseInt(value.slice(2, 4), 16), parseInt(value.slice(4, 6), 16)];
}

function makeCanvas(width, height) {
  return Buffer.alloc(width * height * 4);
}

function setPixel(canvas, width, x, y, r, g, b, a) {
  const offset = (y * width + x) * 4;
  canvas[offset] = r;
  canvas[offset + 1] = g;
  canvas[offset + 2] = b;
  canvas[offset + 3] = a;
}

// Круг вписан в квадрат `size × size`; всё за пределами радиуса остаётся прозрачным — так у
// каждой картинки есть собственная прозрачность файла, не только заливка цветом.
function circleImage(size, colorHex, radiusRatio = 0.42) {
  const [r, g, b] = hexToRgb(colorHex);
  const canvas = makeCanvas(size, size);
  const center = (size - 1) / 2;
  const radius = size * radiusRatio;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const dx = x - center;
      const dy = y - center;
      if (dx * dx + dy * dy <= radius * radius) setPixel(canvas, size, x, y, r, g, b, 255);
    }
  }
  return { width: size, height: size, canvas };
}

// Прямоугольник со скруглёнными углами; `alpha` — постоянная прозрачность заливки (панель
// демо-игры полупрозрачна, как и её цветная версия в screens.json).
function roundedRectImage(width, height, colorHex, alpha, cornerRadius) {
  const [r, g, b] = hexToRgb(colorHex);
  const canvas = makeCanvas(width, height);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (insideRoundedRect(x, y, width, height, cornerRadius)) setPixel(canvas, width, x, y, r, g, b, alpha);
    }
  }
  return { width, height, canvas };
}

function insideRoundedRect(x, y, width, height, cornerRadius) {
  const cx = x < cornerRadius ? cornerRadius : x > width - 1 - cornerRadius ? width - 1 - cornerRadius : x;
  const cy = y < cornerRadius ? cornerRadius : y > height - 1 - cornerRadius ? height - 1 - cornerRadius : y;
  const dx = x - cx;
  const dy = y - cy;
  return dx * dx + dy * dy <= cornerRadius * cornerRadius;
}

// Лента кадров слева направо, все одной ширины — «Картинки» → пункт 2: `frames` кадров
// `frameSize × frameSize` каждый, склеенных в одну картинку шириной `frames * frameSize`.
function framesStrip(frameSize, frames, drawFrame) {
  const canvas = makeCanvas(frameSize * frames, frameSize);
  for (let frame = 0; frame < frames; frame++) {
    const { canvas: frameCanvas } = drawFrame(frame);
    for (let y = 0; y < frameSize; y++) {
      const srcStart = y * frameSize * 4;
      const destStart = (y * frameSize * frames + frame * frameSize) * 4;
      frameCanvas.copy(canvas, destStart, srcStart, srcStart + frameSize * 4);
    }
  }
  return { width: frameSize * frames, height: frameSize, canvas };
}

// Еда и мяч «дышат»: радиус мягко колеблется по кадрам, а не просто включается-выключается —
// так лента заметно живая, но кадр по-прежнему выбирается делением на шаги/время, без домножения.
function pulsingCircle(size, colorHex, frame, frameCount) {
  const phase = (frame / frameCount) * Math.PI * 2;
  return circleImage(size, colorHex, 0.42 + 0.08 * Math.sin(phase));
}

const IMAGES = {
  "snake/images/head.png": () => circleImage(CELL, "#5ad469"),
  "snake/images/tail.png": () => circleImage(CELL, "#2f7a3d"),
  "snake/images/food.png": () => framesStrip(CELL, 4, (frame) => pulsingCircle(CELL, "#e05a5a", frame, 4)),
  "snake/images/panel.png": () => roundedRectImage(256, 64, "#12141a", 0xcc, 14),
  "snake/images/button.png": () => roundedRectImage(256, 64, "#5ad469", 0xff, 14),
  "snake/images/button_hover.png": () => roundedRectImage(256, 64, "#6fe07d", 0xff, 14),
  "snake/images/button_pressed.png": () => roundedRectImage(256, 64, "#3aa54c", 0xff, 14),

  "arkanoid/images/paddle.png": () => roundedRectImage(CELL * 4, CELL, "#c8ccd4", 0xff, 8),
  "arkanoid/images/ball.png": () => framesStrip(CELL, 4, (frame) => pulsingCircle(CELL, "#f2f2f2", frame, 4)),
  "arkanoid/images/brick_red.png": () => roundedRectImage(CELL * 2, CELL, "#e05a5a", 0xff, 4),
  "arkanoid/images/brick_amber.png": () => roundedRectImage(CELL * 2, CELL, "#e0a75a", 0xff, 4),
  "arkanoid/images/brick_yellow.png": () => roundedRectImage(CELL * 2, CELL, "#e0d65a", 0xff, 4),
};

for (const [relativePath, draw] of Object.entries(IMAGES)) {
  const target = resolve(gamesDir, relativePath);
  mkdirSync(dirname(target), { recursive: true });
  const { width, height, canvas } = draw();
  const png = encodePng(width, height, canvas);
  writeFileSync(target, png);
  console.log(`${relativePath}: ${width}×${height}, ${png.length} байт`);
}
