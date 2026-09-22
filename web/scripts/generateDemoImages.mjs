import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

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
export function encodePng(width, height, rgba) {
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

export function hexToRgb(hex) {
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

// Лента кадров слева направо, все одной высоты — «Картинки» → пункт 2: `frames` кадров
// `frameWidth × frameHeight` каждый, склеенных в одну картинку шириной `frames * frameWidth`.
export function framesStrip(frameWidth, frameHeight, frames, drawFrame) {
  const canvas = makeCanvas(frameWidth * frames, frameHeight);
  for (let frame = 0; frame < frames; frame++) {
    const { canvas: frameCanvas } = drawFrame(frame);
    for (let y = 0; y < frameHeight; y++) {
      const srcStart = y * frameWidth * 4;
      const destStart = (y * frameWidth * frames + frame * frameWidth) * 4;
      frameCanvas.copy(canvas, destStart, srcStart, srcStart + frameWidth * 4);
    }
  }
  return { width: frameWidth * frames, height: frameHeight, canvas };
}

// Еда и мяч «дышат»: радиус мягко колеблется по кадрам, а не просто включается-выключается —
// так лента заметно живая, но кадр по-прежнему выбирается делением на шаги/время, без домножения.
function pulsingCircle(size, colorHex, frame, frameCount) {
  const phase = (frame / frameCount) * Math.PI * 2;
  return circleImage(size, colorHex, 0.42 + 0.08 * Math.sin(phase));
}

function lighten([r, g, b], factor) {
  return [r, g, b].map((c) => Math.min(255, Math.round(c + (255 - c) * factor)));
}

function darken([r, g, b], factor) {
  return [r, g, b].map((c) => Math.round(c * (1 - factor)));
}

// Кубик с объёмом: фаска «пирамидой» — верхняя и левая грань светлее, нижняя и правая темнее,
// как у блока в NES Tetris. Каждый пиксель относят к ближайшей грани по расстоянию до неё.
export function bevelCubeImage(size, colorHex) {
  const base = hexToRgb(colorHex);
  const light = lighten(base, 0.4);
  const dark = darken(base, 0.35);
  const bevel = Math.round(size * 0.2);
  const canvas = makeCanvas(size, size);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const toTop = y;
      const toLeft = x;
      const toBottom = size - 1 - y;
      const toRight = size - 1 - x;
      let [r, g, b] = base;
      if (toTop < bevel && toTop <= toLeft && toTop <= toRight) [r, g, b] = light;
      else if (toLeft < bevel && toLeft <= toTop && toLeft <= toBottom) [r, g, b] = light;
      else if (toBottom < bevel && toBottom <= toLeft && toBottom <= toRight) [r, g, b] = dark;
      else if (toRight < bevel && toRight <= toTop && toRight <= toBottom) [r, g, b] = dark;
      setPixel(canvas, size, x, y, r, g, b, 255);
    }
  }
  return { width: size, height: size, canvas };
}

// Сплошная заливка постоянного цвета и прозрачности — для `blank.png` (alpha 0) и кадров
// `flash.png` (alpha 0 и 0xff).
function solidImage(width, height, colorHex, alpha) {
  const [r, g, b] = hexToRgb(colorHex);
  const canvas = makeCanvas(width, height);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) setPixel(canvas, width, x, y, r, g, b, alpha);
  }
  return { width, height, canvas };
}

// Голова змейки смотрит вправо при `rotation: 0` — движок поворачивает картинку по направлению
// («Змейка» фазы 06), так что асимметрия (глаза у правого края) обязательна: круглая картинка
// без неё выглядела бы одинаково при любом повороте.
function snakeHeadImage(size, colorHex) {
  const { canvas } = circleImage(size, colorHex);
  const eyeColor = darken(hexToRgb(colorHex), 0.7);
  const eyeX = size * 0.62;
  const eyeRadius = size * 0.08;
  for (const eyeY of [size * 0.32, size * 0.68]) {
    for (let y = 0; y < size; y++) {
      for (let x = 0; x < size; x++) {
        const dx = x - eyeX;
        const dy = y - eyeY;
        if (dx * dx + dy * dy <= eyeRadius * eyeRadius) {
          setPixel(canvas, size, x, y, eyeColor[0], eyeColor[1], eyeColor[2], 255);
        }
      }
    }
  }
  return { width: size, height: size, canvas };
}

// Отрезок по алгоритму Брезенхэма, для трещин на кирпиче.
function drawLine(canvas, width, height, x0, y0, x1, y1, colorRgb) {
  let x = Math.round(x0);
  let y = Math.round(y0);
  const endX = Math.round(x1);
  const endY = Math.round(y1);
  const dx = Math.abs(endX - x);
  const sx = x < endX ? 1 : -1;
  const dy = -Math.abs(endY - y);
  const sy = y < endY ? 1 : -1;
  let err = dx + dy;
  for (;;) {
    if (x >= 0 && x < width && y >= 0 && y < height) {
      setPixel(canvas, width, x, y, colorRgb[0], colorRgb[1], colorRgb[2], 255);
    }
    if (x === endX && y === endY) break;
    const e2 = 2 * err;
    if (e2 >= dy) {
      err += dy;
      x += sx;
    }
    if (e2 <= dx) {
      err += dx;
      y += sy;
    }
  }
}

function drawPolyline(canvas, width, height, points, colorRgb) {
  for (let i = 0; i + 1 < points.length; i++) {
    const [x0, y0] = points[i];
    const [x1, y1] = points[i + 1];
    drawLine(canvas, width, height, x0, y0, x1, y1, colorRgb);
  }
}

// Две трещины кирпича 64 × 32 («Файлы, которые делают скрипты»): вторая копится поверх первой,
// так третий кадр выглядит сильнее потрескавшимся, а не просто другим узором.
const BRICK_CRACK_MAIN = [
  [30, 2],
  [25, 11],
  [34, 17],
  [27, 25],
  [31, 30],
];
const BRICK_CRACK_EXTRA = [
  [46, 6],
  [39, 14],
  [48, 20],
  [41, 28],
];

export function crackedBrickFrame(width, height, colorHex, frame) {
  const { canvas } = roundedRectImage(width, height, colorHex, 0xff, 4);
  const crackColor = darken(hexToRgb(colorHex), 0.55);
  if (frame >= 1) drawPolyline(canvas, width, height, BRICK_CRACK_MAIN, crackColor);
  if (frame >= 2) drawPolyline(canvas, width, height, BRICK_CRACK_EXTRA, crackColor);
  return { width, height, canvas };
}

// Цвет по фигуре — «Картинки» → пункт 34: у каждой фигуры тетриса свой цвет (палитра
// Tetris Guideline: I голубой, O жёлтый, T фиолетовый, S зелёный, Z красный, J синий, L оранжевый).
const TETRIS_CUBE_COLORS = {
  cube_i: "#4dd9ec",
  cube_o: "#e6d94d",
  cube_t: "#b06fe0",
  cube_s: "#5ad469",
  cube_z: "#e05a5a",
  cube_j: "#4d7de0",
  cube_l: "#e08a3d",
};

// Экспортируется, чтобы тест сверял размер и число кадров каждого файла с договором раздела
// «Файлы, которые делают скрипты», не запуская запись на диск.
export const IMAGES = {
  "snake/images/head.png": () => snakeHeadImage(CELL, "#5ad469"),
  "snake/images/tail.png": () => circleImage(CELL, "#2f7a3d"),
  "snake/images/food.png": () => framesStrip(CELL, CELL, 4, (frame) => pulsingCircle(CELL, "#e05a5a", frame, 4)),
  "snake/images/panel.png": () => roundedRectImage(256, 64, "#12141a", 0xcc, 14),
  "snake/images/button.png": () => roundedRectImage(256, 64, "#5ad469", 0xff, 14),
  "snake/images/button_hover.png": () => roundedRectImage(256, 64, "#6fe07d", 0xff, 14),
  "snake/images/button_pressed.png": () => roundedRectImage(256, 64, "#3aa54c", 0xff, 14),

  "arkanoid/images/paddle.png": () => roundedRectImage(CELL * 4, CELL, "#c8ccd4", 0xff, 8),
  "arkanoid/images/ball.png": () => framesStrip(CELL, CELL, 4, (frame) => pulsingCircle(CELL, "#f2f2f2", frame, 4)),
  "arkanoid/images/brick_red.png": () =>
    framesStrip(CELL * 2, CELL, 3, (frame) => crackedBrickFrame(CELL * 2, CELL, "#e05a5a", frame)),
  "arkanoid/images/brick_amber.png": () => roundedRectImage(CELL * 2, CELL, "#e0a75a", 0xff, 4),
  "arkanoid/images/brick_yellow.png": () => roundedRectImage(CELL * 2, CELL, "#e0d65a", 0xff, 4),

  "tetris/images/wall.png": () => bevelCubeImage(CELL, "#6b7280"),
  "tetris/images/flash.png": () =>
    framesStrip(CELL, CELL, 2, (frame) => solidImage(CELL, CELL, "#ffffff", frame === 0 ? 0xff : 0x00)),
  "tetris/images/blank.png": () => solidImage(CELL, CELL, "#000000", 0x00),
};

for (const [name, colorHex] of Object.entries(TETRIS_CUBE_COLORS)) {
  IMAGES[`tetris/images/${name}.png`] = () => bevelCubeImage(CELL, colorHex);
}

// Модуль импортируется тестом напрямую (см. generateDemoImages.test.mjs) — без этой проверки
// такой импорт тут же перезаписывал бы настоящие картинки игр.
const isMainModule = import.meta.url === pathToFileURL(process.argv[1] ?? "").href;
if (isMainModule) {
  for (const [relativePath, draw] of Object.entries(IMAGES)) {
    const target = resolve(gamesDir, relativePath);
    mkdirSync(dirname(target), { recursive: true });
    const { width, height, canvas } = draw();
    const png = encodePng(width, height, canvas);
    writeFileSync(target, png);
    console.log(`${relativePath}: ${width}×${height}, ${png.length} байт`);
  }
}
