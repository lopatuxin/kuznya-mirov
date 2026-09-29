import { describe, expect, it } from "vitest";
import { buildBottomAlignedStrip, columnBottoms, diagonalStaircaseCells, mergeCellsIntoRects, mergeDiagonalStaircase } from "./wallStrip.mjs";

// Картинка `width`×`height`, непрозрачная точка (255) там, где `isOpaque(x, y)` истинно.
function image(width, height, isOpaque) {
  const rgba = Buffer.alloc(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (!isOpaque(x, y)) continue;
      const i = (y * width + x) * 4;
      rgba[i] = rgba[i + 1] = rgba[i + 2] = rgba[i + 3] = 255;
    }
  }
  return rgba;
}

describe("columnBottoms", () => {
  it("низ столбца — нижняя непрозрачная строка, у разных столбцов разная (требование 13)", () => {
    // Диагональ: столбец x непрозрачен только в строке x (2 столбца, высота 2).
    const rgba = image(2, 2, (x, y) => x === y);
    expect(columnBottoms(rgba, 2, 2, 1)).toEqual([0, 1]);
  });

  it("целиком прозрачный столбец — -1", () => {
    const rgba = image(2, 2, () => false);
    expect(columnBottoms(rgba, 2, 2, 1)).toEqual([-1, -1]);
  });

  it("столбец шириной больше одной точки — по любой непрозрачной точке в полосе", () => {
    const rgba = image(4, 3, (x, y) => x === 3 && y === 2); // только одна точка, во втором столбце (x∈[2,4))
    expect(columnBottoms(rgba, 4, 3, 2)).toEqual([-1, 2]);
  });
});

describe("buildBottomAlignedStrip", () => {
  it("кадр — тот же столбец, сдвинутый так, что его низ на нижней строке кадра", () => {
    // Столбец 0 непрозрачен в строке 0 из 3 (высоко), столбец 1 — в строке 2 из 3 (уже внизу).
    const rgba = image(2, 3, (x, y) => (x === 0 && y === 0) || (x === 1 && y === 2));
    const bottoms = columnBottoms(rgba, 2, 3, 1);
    expect(bottoms).toEqual([0, 2]);
    const strip = buildBottomAlignedStrip(rgba, 2, 3, 1, bottoms);
    expect(strip.width).toBe(2);
    expect(strip.height).toBe(3);
    // Кадр 0 (столбец 0) — точка была в строке 0, сдвиг 3-1-0=2, теперь должна быть в строке 2.
    expect(strip.data[(2 * strip.width + 0) * 4 + 3]).toBe(255);
    expect(strip.data[(0 * strip.width + 0) * 4 + 3]).toBe(0);
    // Кадр 1 (столбец 1) — точка была в строке 2 (уже низ), сдвиг 0, остаётся в строке 2.
    expect(strip.data[(2 * strip.width + 1) * 4 + 3]).toBe(255);
  });

  it("прозрачный столбец — прозрачный кадр целиком", () => {
    const rgba = image(2, 2, (x) => x === 0);
    const bottoms = columnBottoms(rgba, 2, 2, 1);
    const strip = buildBottomAlignedStrip(rgba, 2, 2, 1, bottoms);
    for (let y = 0; y < 2; y++) expect(strip.data[(y * strip.width + 1) * 4 + 3]).toBe(0);
  });

  it("все кадры одной высоты, равной высоте исходной картинки («Картинки»: все кадры одного размера)", () => {
    const rgba = image(3, 5, (x, y) => y === x); // только для x < 5
    const bottoms = columnBottoms(rgba, 3, 5, 1);
    const strip = buildBottomAlignedStrip(rgba, 3, 5, 1, bottoms);
    expect(strip.height).toBe(5);
    expect(strip.frameCount).toBe(3);
  });
});

describe("diagonalStaircaseCells", () => {
  it("две клетки на восток, потом одна на север — по формуле требования 2", () => {
    expect(diagonalStaircaseCells(5)).toEqual([
      { x: 0, y: 0 },
      { x: 1, y: 0 },
      { x: 2, y: -1 },
      { x: 3, y: -1 },
      { x: 4, y: -2 },
    ]);
  });
});

describe("mergeCellsIntoRects", () => {
  it("порядок клеток на входе не важен — сливает по строке независимо от него", () => {
    const cells = [
      { x: 5, y: 0 },
      { x: 3, y: 0 },
      { x: 4, y: 0 },
      { x: 3, y: -1 },
    ];
    expect(mergeCellsIntoRects(cells)).toEqual([
      { x: 3, y: 0, width: 3, height: 1 },
      { x: 3, y: -1, width: 1, height: 1 },
    ]);
  });

  it("разрыв в столбцах — два отдельных прямоугольника в одной строке", () => {
    const cells = [
      { x: 0, y: 0 },
      { x: 1, y: 0 },
      { x: 5, y: 0 },
    ];
    expect(mergeCellsIntoRects(cells)).toEqual([
      { x: 0, y: 0, width: 2, height: 1 },
      { x: 5, y: 0, width: 1, height: 1 },
    ]);
  });
});

describe("mergeDiagonalStaircase", () => {
  it("сливает пары в одной строке в прямоугольник 2×1", () => {
    expect(mergeDiagonalStaircase(4)).toEqual([
      { x: 0, y: 0, width: 2, height: 1 },
      { x: 2, y: -1, width: 2, height: 1 },
    ]);
  });

  it("нечётный хвост остаётся прямоугольником 1×1", () => {
    expect(mergeDiagonalStaircase(3)).toEqual([
      { x: 0, y: 0, width: 2, height: 1 },
      { x: 2, y: -1, width: 1, height: 1 },
    ]);
  });

  it("площадь прямоугольников равна числу столбцов — ни один не потерян и не задвоен", () => {
    for (let n = 1; n <= 11; n++) {
      const rects = mergeDiagonalStaircase(n);
      const area = rects.reduce((sum, r) => sum + r.width * r.height, 0);
      expect(area).toBe(n);
    }
  });
});
