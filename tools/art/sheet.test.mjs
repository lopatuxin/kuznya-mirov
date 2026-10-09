import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";
import ffmpegPath from "ffmpeg-static";
import sharp from "sharp";
import {
  bleedColor, fadeEnds, findCycle, findLoop, keyGreen, keyMagenta, keyVideoFrame, makeVideo, padToEven, parseRange,
  pickFrames, stackColorAndMask, wrapEnds,
} from "./sheet.mjs";

const THUMB = 64;

// Эскиз кадра — ровная заливка яркостью `level`: разность двух эскизов равна разности яркостей.
function flat(level) {
  return new Uint8Array(THUMB).fill(level);
}

describe("parseRange", () => {
  it("переводит кадры A-B, считая с единицы, в начало и длину, считая с нуля", () => {
    assert.deepEqual(parseRange("3-5"), { start: 2, length: 3 });
    assert.deepEqual(parseRange("7-7"), { start: 6, length: 1 });
  });

  it("отвергает нулевой кадр, обратный порядок и не число", () => {
    for (const text of ["0-4", "5-3", "abc", "3", "1-2-3"]) {
      assert.throws(() => parseRange(text), /--range/);
    }
  });
});

describe("keyGreen", () => {
  it("чистый зелёный фон становится прозрачным", () => {
    const rgba = keyGreen(Buffer.from([10, 250, 5]));
    assert.equal(rgba[3], 0);
  });

  it("светлая кость остаётся непрозрачной и своего цвета", () => {
    assert.deepEqual([...keyGreen(Buffer.from([240, 225, 200]))], [240, 225, 200, 255]);
  });

  it("край получает промежуточную прозрачность, а зелёный прижимается к красному и синему", () => {
    const [red, green, blue, alpha] = keyGreen(Buffer.from([100, 170, 60]));
    assert.deepEqual([red, green, blue], [100, 100, 60]);
    assert.ok(alpha > 0 && alpha < 255, `прозрачность ${alpha}`);
  });
});

describe("keyMagenta", () => {
  it("чистый розовый фон становится прозрачным", () => {
    const rgba = keyMagenta(Buffer.from([250, 10, 245]));
    assert.equal(rgba[3], 0);
  });

  it("трава и бурая земля остаются непрозрачными и своего цвета", () => {
    assert.deepEqual([...keyMagenta(Buffer.from([80, 140, 40]))], [80, 140, 40, 255]);
    assert.deepEqual([...keyMagenta(Buffer.from([200, 150, 90]))], [200, 150, 90, 255]);
  });

  it("край получает промежуточную прозрачность, а красный и синий снижаются на избыток над зелёным", () => {
    const [red, green, blue, alpha] = keyMagenta(Buffer.from([180, 100, 170]));
    assert.deepEqual([red, green, blue], [110, 100, 100]);
    assert.ok(alpha > 0 && alpha < 255, `прозрачность ${alpha}`);
  });
});

describe("keyVideoFrame", () => {
  // Десять точек розового фона яркостью 102 и десять точек травы яркостью 30 задают уровни ключа.
  const BACKGROUND = [226, 45, 225];
  const GRASS = [40, 90, 30];
  function frame(extra) {
    const pixels = [...Array(10).fill(BACKGROUND), ...Array(10).fill(GRASS), ...extra.map(([color]) => color)];
    const luma = [...Array(10).fill(102), ...Array(10).fill(30), ...extra.map(([, level]) => level)];
    return keyVideoFrame(Buffer.from(pixels.flat()), Uint8Array.from(luma), "magenta");
  }
  const alphaAt = (rgba, index) => rgba[index * 4 + 3];

  it("тёмный стебель розового цвета с яркостью травы получает полную непрозрачность", () => {
    assert.equal(alphaAt(frame([[[150, 40, 140], 30]]), 20), 255);
  });

  it("чистый розовый фон остаётся прозрачным", () => {
    assert.equal(alphaAt(frame([[BACKGROUND, 102]]), 20), 0);
  });

  it("точка ровно посередине яркостей фона и травы — наполовину прозрачная", () => {
    const alpha = alphaAt(frame([[BACKGROUND, 66]]), 20);
    assert.ok(Math.abs(alpha - 128) <= 1, `прозрачность ${alpha}`);
  });

  it("у краёв промежутка яркостей — уже 0 и 1", () => {
    // Промежуток 72, его десятая доля — 7,2: яркость 37 уже трава, 95 — ещё фон.
    const rgba = frame([[BACKGROUND, 37], [BACKGROUND, 95]]);
    assert.equal(alphaAt(rgba, 20), 255);
    assert.equal(alphaAt(rgba, 21), 0);
  });

  const colorAt = (rgba, index) => [...rgba.subarray(index * 4, index * 4 + 3)];
  const lumaOf = ([red, green, blue]) => 0.299 * red + 0.587 * green + 0.114 * blue;

  it("розовая примесь к оливковому колоску снимается, яркость точки та же", () => {
    // Оливковый (100, 85, 50) с седьмой долей розового фона: (119, 79, 76). Ключ по цвету оставил бы его красноватым.
    const mixed = [119, 79, 76];
    const [red, green, blue] = colorAt(frame([[mixed, 30]]), 20);
    assert.ok(Math.abs(lumaOf([red, green, blue]) - lumaOf(mixed)) <= 1, `яркость ${lumaOf([red, green, blue])}`);
    assert.ok(red - green <= 25, `не красный: ${[red, green, blue]}`);
    assert.ok(blue < green - 10, `не розовый: ${[red, green, blue]}`);
  });

  it("полупрозрачный край из травы и фона получает яркость травы, а не светлой смеси", () => {
    // Поровну трава (40, 90, 30) и розовый фон: смесь (133, 68, 128) по яркости светлее травы на 26.
    const mixed = [133, 68, 128];
    const color = colorAt(frame([[mixed, 66]]), 20);
    assert.ok(Math.abs(lumaOf(color) - lumaOf(GRASS)) < 15, `край ${color}, яркость ${lumaOf(color)}`);
  });

  it("самый внешний край, где травы пятая доля, не уходит в чёрное", () => {
    // Ключ по яркости даёт такой точке прозрачность меньше её доли травы, и вычитание фона по ней ушло бы ниже травы.
    const mixed = [189, 54, 186];
    const color = colorAt(frame([[mixed, 88]]), 20);
    assert.ok(Math.abs(lumaOf(color) - lumaOf(GRASS)) < 15, `край ${color}, яркость ${lumaOf(color)}`);
  });

  it("зелёная трава своего цвета не меняет", () => {
    const color = colorAt(frame([]), 10);
    color.forEach((channel, c) => assert.ok(Math.abs(channel - GRASS[c]) <= 1, `трава ${color}`));
  });
});

describe("findLoop", () => {
  it("находит длину повтора искусственной последовательности", () => {
    const masks = Array.from({ length: 120 }, (_, i) => flat((i % 40) * 6));
    assert.equal(findLoop(masks).length, 40);
  });

  it("не берёт петлю короче полутора секунд: повтор через 20 кадров даёт петлю в два повтора", () => {
    const masks = Array.from({ length: 80 }, (_, i) => flat((i % 20) * 10));
    assert.equal(findLoop(masks).length, 40);
  });

  it("сообщает, что кадров на петлю мало", () => {
    assert.throws(() => findLoop(Array.from({ length: 36 }, () => flat(0))), /--range/);
  });
});

describe("bleedColor", () => {
  it("продолжает цвет видимой точки под прозрачные на 8 точек, дальше чёрный", () => {
    const rgba = Buffer.alloc(12 * 4);
    rgba.set([200, 100, 50, 255], 0);
    for (let x = 1; x < 12; x++) rgba.set([226, 45, 225, 0], x * 4);
    bleedColor(rgba, 12, 1);
    const colorAt = (x) => [...rgba.subarray(x * 4, x * 4 + 4)];
    assert.deepEqual(colorAt(0), [200, 100, 50, 255]);
    assert.deepEqual(colorAt(1), [200, 100, 50, 0]);
    assert.deepEqual(colorAt(8), [200, 100, 50, 0]);
    assert.deepEqual(colorAt(9), [0, 0, 0, 0]);
  });
});

describe("padToEven", () => {
  it("дополняет нечётные стороны прозрачной строкой сверху и столбцом справа", () => {
    const rgba = Buffer.from([...Array(3)].flatMap(() => [10, 20, 30, 255]));
    const out = padToEven(rgba, 3, 1);
    assert.equal(out.width, 4);
    assert.equal(out.height, 2);
    assert.deepEqual([...out.rgba.subarray(0, 16)], Array(16).fill(0));
    assert.deepEqual([...out.rgba.subarray(16, 20)], [10, 20, 30, 255]);
    assert.deepEqual([...out.rgba.subarray(28, 32)], [0, 0, 0, 0]);
  });
});

describe("stackColorAndMask", () => {
  it("кладёт цвет сверху, а прозрачность серым под ним", () => {
    const rgba = Buffer.from([200, 100, 50, 255, 7, 8, 9, 0]);
    assert.deepEqual([...stackColorAndMask(rgba, 2, 1)], [200, 100, 50, 7, 8, 9, 255, 255, 255, 0, 0, 0]);
  });
});

describe("makeVideo", () => {
  it("делает видео двойной высоты с чётными половинами и маской травы снизу", async () => {
    const dir = mkdtempSync(join(tmpdir(), "kuznya-video-test-"));
    try {
      // Тёмно-зелёный прямоугольник нечётного размера на розовом фоне, 48 кадров.
      const clip = join(dir, "clip.mp4");
      const make = spawnSync(ffmpegPath, [
        "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=0xE22DE1:s=64x48:r=24:d=2",
        "-vf", "drawbox=x=21:y=15:w=21:h=11:color=0x1E3C14:t=fill", "-c:v", "libx264", "-pix_fmt", "yuv420p", clip,
      ], { encoding: "utf8" });
      assert.equal(make.status, 0, make.stderr);
      const output = join(dir, "out.mp4");
      await makeVideo(clip, output, { key: "magenta", range: parseRange("1-40") });

      const first = join(dir, "first.png");
      const decode = spawnSync(ffmpegPath, ["-hide_banner", "-loglevel", "error", "-i", output, "-frames:v", "1", first], { encoding: "utf8" });
      assert.equal(decode.status, 0, decode.stderr);
      const { data, info } = await sharp(first).removeAlpha().raw().toBuffer({ resolveWithObject: true });
      const half = info.height / 2;
      assert.equal(info.width % 2, 0);
      assert.equal(half % 2, 0);
      const red = (x, y) => data[(y * info.width + x) * 3];
      assert.ok(red(info.width / 2, half + half / 2) > 230, `маска травы ${red(info.width / 2, half + half / 2)}`);
      assert.ok(red(1, half + 1) < 30, `маска фона ${red(1, half + 1)}`);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("fadeEnds", () => {
  it("гасит концы от крайних видимых точек, середину и прозрачные поля не трогает", () => {
    const alphas = [0, 255, 255, 255, 255, 255, 255, 255, 255, 0];
    const rgba = Buffer.from(alphas.flatMap((alpha) => [100, 80, 60, alpha]));
    fadeEnds(rgba, alphas.length, 1, 3);
    const faded = alphas.map((_, x) => rgba[x * 4 + 3]);
    assert.equal(faded[0], 0);
    assert.equal(faded[1], 0);
    assert.ok(faded[2] > 0 && faded[2] < 255, `прозрачность ${faded[2]}`);
    assert.equal(faded[4], 255);
    assert.equal(faded[5], 255);
    assert.equal(faded[8], 0);
    assert.equal(faded[9], 0);
  });
});

describe("wrapEnds", () => {
  it("укорачивает картинку на наложение и ставит в начало конец исходника, чтобы повтор шёл без шва", () => {
    const reds = [0, 10, 20, 30, 40, 50, 60, 70, 80, 90];
    const rgba = Buffer.from(reds.flatMap((red) => [red, 0, 0, 255]));
    const out = wrapEnds(rgba, reds.length, 1, 3);
    const outReds = Array.from({ length: 7 }, (_, x) => out[x * 4]);
    assert.equal(outReds[0], 70);
    assert.deepEqual(outReds.slice(3), [30, 40, 50, 60]);
    assert.ok(outReds[1] < 80 && outReds[1] > 10, `смешанная точка ${outReds[1]}`);
  });

  it("прозрачная точка не темнит видимую, с которой смешивается", () => {
    // Вторая точка результата — поровну вторая точка исходника и прозрачная последняя.
    const pixels = [[0, 0, 0, 255], [200, 100, 50, 255], [0, 0, 0, 255], [0, 0, 0, 255], [0, 0, 0, 0]];
    const out = wrapEnds(Buffer.from(pixels.flat()), pixels.length, 1, 2);
    assert.deepEqual([...out.subarray(4, 8)], [200, 100, 50, 128]);
  });
});

describe("findCycle", () => {
  it("находит длину повторяющегося движения", () => {
    const thumbs = Array.from({ length: 120 }, (_, i) => flat((i % 24) * 10));
    assert.equal(findCycle(thumbs).length, 24);
  });

  it("не принимает половину шага за целый, когда шаг левой и правой ногой лишь похожи", () => {
    // Вторая половина цикла повторяет первую с небольшим сдвигом яркости: как шаг другой ногой.
    const thumbs = Array.from({ length: 120 }, (_, i) => flat((i % 12) * 20 + (i % 24 < 12 ? 0 : 6)));
    assert.equal(findCycle(thumbs).length, 24);
  });

  it("сообщает, что повтора нет, если кадры не повторяются", () => {
    let seed = 20260928;
    const thumbs = Array.from({ length: 120 }, () => {
      seed = (seed * 48271) % 2147483647;
      return flat(seed % 256);
    });
    assert.throws(() => findCycle(thumbs), /--range/);
  });
});

describe("pickFrames", () => {
  it("берёт каждый второй кадр, когда нужно вдвое меньше", () => {
    assert.deepEqual(pickFrames({ start: 10, length: 8 }, 4), [10, 12, 14, 16]);
  });

  it("берёт все кадры отрезка, когда нужно столько же", () => {
    assert.deepEqual(pickFrames({ start: 0, length: 3 }, 3), [0, 1, 2]);
  });
});
