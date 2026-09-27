import { describe, expect, it } from "vitest";
import { IMAGES, bevelCubeImage, crackedBrickFrame, encodePng, framesStrip, hexToRgb } from "./generateDemoImages.mjs";

function readPixel(canvas, width, x, y) {
  const offset = (y * width + x) * 4;
  return [canvas[offset], canvas[offset + 1], canvas[offset + 2], canvas[offset + 3]];
}

describe("hexToRgb", () => {
  it("разбирает цвет по каналам", () => {
    expect(hexToRgb("#5ad469")).toEqual([0x5a, 0xd4, 0x69]);
  });
});

describe("encodePng", () => {
  it("начинается с подписи PNG и несёт правильные ширину и высоту в IHDR", () => {
    const canvas = Buffer.alloc(4 * 3 * 4);
    const png = encodePng(4, 3, canvas);
    expect(png.subarray(0, 8)).toEqual(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]));
    expect(png.readUInt32BE(16)).toBe(4); // ширина в IHDR
    expect(png.readUInt32BE(20)).toBe(3); // высота в IHDR
  });
});

describe("framesStrip", () => {
  it("склеивает кадры разной ширины и высоты слева направо без смешения пикселей", () => {
    const { width, height, canvas } = framesStrip(2, 3, 2, (frame) => {
      const frameCanvas = Buffer.alloc(2 * 3 * 4);
      for (let i = 0; i < 2 * 3; i++) frameCanvas[i * 4] = frame === 0 ? 10 : 20;
      return { canvas: frameCanvas };
    });
    expect(width).toBe(4);
    expect(height).toBe(3);
    expect(readPixel(canvas, width, 0, 0)[0]).toBe(10);
    expect(readPixel(canvas, width, 1, 2)[0]).toBe(10);
    expect(readPixel(canvas, width, 2, 0)[0]).toBe(20);
    expect(readPixel(canvas, width, 3, 2)[0]).toBe(20);
  });
});

describe("bevelCubeImage", () => {
  it("верхний-левый угол светлее заливки, нижний-правый — темнее", () => {
    const { width, canvas } = bevelCubeImage(32, "#808080");
    const base = hexToRgb("#808080");
    const topLeft = readPixel(canvas, width, 0, 0);
    const bottomRight = readPixel(canvas, width, 31, 31);
    const center = readPixel(canvas, width, 16, 16);
    expect(topLeft[0]).toBeGreaterThan(base[0]);
    expect(bottomRight[0]).toBeLessThan(base[0]);
    expect(center.slice(0, 3)).toEqual(base);
    expect(topLeft[3]).toBe(255); // непрозрачен целиком, в отличие от circleImage
  });
});

describe("crackedBrickFrame", () => {
  it("кадр 0 — без трещин, кадр 2 темнее в их точках, чем кадр 0", () => {
    const width = 64;
    const height = 32;
    const intact = crackedBrickFrame(width, height, "#e05a5a", 0);
    const badlyCracked = crackedBrickFrame(width, height, "#e05a5a", 2);
    const [x, y] = [30, 2]; // первая точка BRICK_CRACK_MAIN
    const intactPixel = readPixel(intact.canvas, width, x, y);
    const crackedPixel = readPixel(badlyCracked.canvas, width, x, y);
    expect(crackedPixel[0]).toBeLessThan(intactPixel[0]);
  });
});

// «Файлы, которые делают скрипты» — договор с данными игр: эти размеры и число кадров менять
// нельзя без правки игровых JSON, поэтому тест сверяет их напрямую, без записи на диск.
describe("IMAGES — договор с данными игр", () => {
  const expectedSizes = {
    "snake/images/head.png": [32, 32],
    "arkanoid/images/brick_red.png": [64 * 3, 32],
    "tetris/images/cube_i.png": [32, 32],
    "tetris/images/cube_o.png": [32, 32],
    "tetris/images/cube_t.png": [32, 32],
    "tetris/images/cube_s.png": [32, 32],
    "tetris/images/cube_z.png": [32, 32],
    "tetris/images/cube_j.png": [32, 32],
    "tetris/images/cube_l.png": [32, 32],
    "tetris/images/wall.png": [32, 32],
    "tetris/images/flash.png": [32 * 2, 32],
    "tetris/images/blank.png": [32, 32],
  };

  it.each(Object.entries(expectedSizes))("%s имеет размер по договору", (path, [width, height]) => {
    const { width: actualWidth, height: actualHeight } = IMAGES[path]();
    expect([actualWidth, actualHeight]).toEqual([width, height]);
  });

  it("blank.png полностью прозрачен", () => {
    const { canvas, width } = IMAGES["tetris/images/blank.png"]();
    expect(readPixel(canvas, width, 0, 0)[3]).toBe(0);
  });
});

// Фаза 01 «Герой ходит по локации», пункты 12–13: 12 кадров по 32×48 в ленте 384×48, сторона
// `Math.floor(frame / 3)`, фаза `frame % 3`; критерии готовности генератора этой фазы.
describe("rpg/images/hero.png — договор с данными игры", () => {
  const FRAME_WIDTH = 32;
  const FRAME_HEIGHT = 48;
  const FRAMES = 12;
  const { width, height, canvas } = IMAGES["rpg/images/hero.png"]();

  function frameBuffer(index) {
    const out = Buffer.alloc(FRAME_WIDTH * FRAME_HEIGHT * 4);
    for (let y = 0; y < FRAME_HEIGHT; y++) {
      const srcStart = (y * width + index * FRAME_WIDTH) * 4;
      canvas.copy(out, y * FRAME_WIDTH * 4, srcStart, srcStart + FRAME_WIDTH * 4);
    }
    return out;
  }

  it("384×48 — 12 кадров по 32×48", () => {
    expect([width, height]).toEqual([FRAME_WIDTH * FRAMES, FRAME_HEIGHT]);
  });

  it("углы каждого кадра прозрачны", () => {
    const corners = [
      [0, 0],
      [FRAME_WIDTH - 1, 0],
      [0, FRAME_HEIGHT - 1],
      [FRAME_WIDTH - 1, FRAME_HEIGHT - 1],
    ];
    for (let index = 0; index < FRAMES; index++) {
      const frame = frameBuffer(index);
      for (const [x, y] of corners) expect(readPixel(frame, FRAME_WIDTH, x, y)[3]).toBe(0);
    }
  });

  it("стоящие кадры четырёх сторон различаются между собой", () => {
    const standingFrames = [0, 3, 6, 9].map(frameBuffer);
    for (let i = 0; i < standingFrames.length; i++) {
      for (let j = i + 1; j < standingFrames.length; j++) {
        expect(standingFrames[i].equals(standingFrames[j])).toBe(false);
      }
    }
  });

  it("кадры шага отличаются от стоящего кадра той же стороны", () => {
    for (const side of [0, 1, 2, 3]) {
      const standing = frameBuffer(side * 3);
      for (const phase of [1, 2]) expect(frameBuffer(side * 3 + phase).equals(standing)).toBe(false);
    }
  });

  it("кадры «вправо» — зеркало кадров «влево» той же фазы", () => {
    for (const phase of [0, 1, 2]) {
      const left = frameBuffer(1 * 3 + phase);
      const right = frameBuffer(2 * 3 + phase);
      for (let y = 0; y < FRAME_HEIGHT; y++) {
        for (let x = 0; x < FRAME_WIDTH; x++) {
          expect(readPixel(right, FRAME_WIDTH, x, y)).toEqual(readPixel(left, FRAME_WIDTH, FRAME_WIDTH - 1 - x, y));
        }
      }
    }
  });

  it("повторный запуск даёт те же байты", () => {
    expect(IMAGES["rpg/images/hero.png"]().canvas.equals(canvas)).toBe(true);
  });
});

// Фаза 01, пункт 17: кольцо отметки щелчка, толщина 3–4 пикселя, внутри и снаружи прозрачно.
describe("rpg/images/marker.png — договор с данными игры", () => {
  const { width, height, canvas } = IMAGES["rpg/images/marker.png"]();

  it("32×32", () => {
    expect([width, height]).toEqual([32, 32]);
  });

  it("середина и углы прозрачны, кольцо непрозрачно", () => {
    expect(readPixel(canvas, width, 16, 16)[3]).toBe(0);
    for (const [x, y] of [
      [0, 0],
      [31, 0],
      [0, 31],
      [31, 31],
    ]) {
      expect(readPixel(canvas, width, x, y)[3]).toBe(0);
    }
    expect(readPixel(canvas, width, 16, 3)[3]).toBe(255); // верхняя точка кольца
  });
});
