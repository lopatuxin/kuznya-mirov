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
