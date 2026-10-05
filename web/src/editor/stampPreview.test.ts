import { describe, expect, it } from "vitest";
import { readStampHeights, shadeStamp } from "./stampPreview";

/** Яркость точки картинки: сумма красного, зелёного и синего. */
function brightness(pixels: Uint8ClampedArray, width: number, x: number, y: number): number {
  const offset = (y * width + x) * 4;
  return (pixels[offset] as number) + (pixels[offset + 1] as number) + (pixels[offset + 2] as number);
}

function alpha(pixels: Uint8ClampedArray, width: number, x: number, y: number): number {
  return pixels[(y * width + x) * 4 + 3] as number;
}

/** Скат по ширине: от 0 слева до 1 справа, в каждой строке одинаково. */
const RISING_RIGHT = [
  [0, 0.5, 1],
  [0, 0.5, 1],
];

describe("readStampHeights", () => {
  it("высоты из файла штампа", () => {
    expect(readStampHeights('{ "heights": [[0, 1], [1, 0]] }')).toEqual([
      [0, 1],
      [1, 0],
    ]);
  });

  it("не JSON, нет высот или меньше двух на двух — картинки нет", () => {
    expect(readStampHeights("{")).toBe(null);
    expect(readStampHeights('{ "heights": [[0, 1]] }')).toBe(null);
    expect(readStampHeights(null)).toBe(null);
  });
});

describe("shadeStamp", () => {
  it("штамп вписан с сохранением пропорций: у узкого штампа поля по бокам прозрачные, середина закрашена", () => {
    const narrow = [
      [0, 0],
      [0, 0],
      [0, 0],
      [0, 0],
    ];
    const pixels = shadeStamp(narrow, 40, 20, 1);

    expect(alpha(pixels, 40, 1, 10)).toBe(0);
    expect(alpha(pixels, 40, 38, 10)).toBe(0);
    expect(alpha(pixels, 40, 20, 10)).toBe(255);
  });

  it("свет слева сверху: склон, обращённый влево, светлее склона, обращённого вправо", () => {
    const facingLeft = shadeStamp(RISING_RIGHT, 30, 20, 1);
    const facingRight = shadeStamp(RISING_RIGHT.map((row) => [...row].reverse()), 30, 20, 1);

    expect(brightness(facingLeft, 30, 15, 10)).toBeGreaterThan(brightness(facingRight, 30, 15, 10));
  });

  it("впадина — та же форма, вдавленная: её склоны освещены наоборот", () => {
    const deepeningRight = shadeStamp(RISING_RIGHT, 30, 20, -1);
    const deepeningLeft = shadeStamp(RISING_RIGHT.map((row) => [...row].reverse()), 30, 20, -1);

    expect(brightness(deepeningLeft, 30, 15, 10)).toBeGreaterThan(brightness(deepeningRight, 30, 15, 10));
  });
});
