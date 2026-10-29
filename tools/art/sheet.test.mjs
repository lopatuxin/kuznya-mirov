import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it } from "node:test";
import ffmpegPath from "ffmpeg-static";
import sharp from "sharp";
import {
  bleedColor, colorEdgesFromFigure, fadeEnds, findCycle, findLoop, keyGreen, keyMagenta, keyPlate, keyVideoFrame, makeVideo,
  padToEven, parseRange, pickFrames, removeBackground, reversePath, stackColorAndMask, wrapEnds,
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

  // Десять точек фона, как у видео syntx: избыток зелёного 240.
  const SCREEN = [0, 240, 0];
  const withScreen = (pixel) => keyGreen(Buffer.from([...Array(10).fill(SCREEN), pixel].flat()));

  it("край получает промежуточную прозрачность, а зелёный прижимается к красному и синему", () => {
    const [red, green, blue, alpha] = withScreen([100, 170, 60]).subarray(10 * 4);
    assert.deepEqual([red, green, blue], [100, 100, 60]);
    assert.ok(alpha > 0 && alpha < 255, `прозрачность ${alpha}`);
  });

  it("чем больше в точке края фона, тем она прозрачнее", () => {
    // Серый (120, 120, 120) без избытка зелёного с четвертью и с половиной фона.
    const quarter = withScreen([90, 150, 90])[10 * 4 + 3];
    const half = withScreen([60, 180, 60])[10 * 4 + 3];
    assert.ok(255 > quarter && quarter > half && half > 0, `четверть ${quarter}, половина ${half}`);
  });

  it("рябь фона видео и тень на фоне от руки остаются прозрачными", () => {
    // Рябь — 0,95 избытка фона (228), тень — 0,76 (182).
    assert.equal(withScreen([5, 233, 5])[10 * 4 + 3], 0);
    assert.equal(withScreen([25, 207, 12])[10 * 4 + 3], 0);
  });

  it("обычный зелёный фон съёмки с избытком 113 тоже становится прозрачным", () => {
    const rgba = keyGreen(Buffer.from([...Array(10).fill([0, 177, 64]), [200, 150, 120]].flat()));
    assert.equal(rgba[3], 0);
    assert.equal(rgba[10 * 4 + 3], 255);
  });

  it("зеленоватое место персонажа с избытком 20 остаётся сплошным, а с избытком 30 уже нет", () => {
    assert.equal(withScreen([100, 120, 100])[10 * 4 + 3], 255);
    assert.ok(withScreen([100, 130, 100])[10 * 4 + 3] < 255);
  });
});

describe("colorEdgesFromFigure", () => {
  // Строка картинки: прозрачность и цвет каждой точки, прочие точки строки прозрачны.
  function row(width, pixels) {
    const rgba = Buffer.alloc(width * 4);
    for (const [x, color] of Object.entries(pixels)) rgba.set(color, Number(x) * 4);
    colorEdgesFromFigure(rgba, width, 1);
    return (x) => [...rgba.subarray(x * 4, x * 4 + 4)];
  }

  it("полупрозрачный край берёт цвет ближайшей сплошной точки, а прозрачность у него прежняя", () => {
    const at = row(3, { 0: [150, 100, 70, 255], 1: [60, 60, 30, 128] });
    assert.deepEqual(at(0), [150, 100, 70, 255]);
    assert.deepEqual(at(1), [150, 100, 70, 128]);
  });

  it("край ровно в четырёх точках от сплошной ещё берёт её цвет", () => {
    assert.deepEqual(row(5, { 0: [150, 100, 70, 255], 4: [60, 60, 30, 40] })(4), [150, 100, 70, 40]);
  });

  it("полупрозрачная точка дальше четырёх точек от места, где персонажа хотя бы половина, — тень на фоне — становится прозрачной", () => {
    assert.equal(row(6, { 0: [150, 100, 70, 255], 5: [25, 25, 12, 40] })(5)[3], 0);
  });

  it("тонкая верёвка с прозрачностью 191 вдали от сплошной точки остаётся своего цвета", () => {
    const rope = Object.fromEntries(Array.from({ length: 9 }, (_, k) => [k + 1, [60, 60, 30, 191]]));
    const at = row(10, { 0: [150, 100, 70, 255], ...rope });
    assert.deepEqual(at(9), [60, 60, 30, 191]);
  });

  it("зеленоватый ремень у руки внутри силуэта не перекрашивается в цвет руки", () => {
    const strap = Object.fromEntries(Array.from({ length: 6 }, (_, k) => [k + 1, [70, 70, 70, 227]]));
    const at = row(7, { 0: [220, 170, 140, 255], ...strap });
    assert.deepEqual(at(1), [70, 70, 70, 227]);
  });

  it("широкий кусок почти сплошной зеленоватой одежды не дырявится и остаётся своего цвета", () => {
    const size = 12;
    const rgba = Buffer.alloc(size * size * 4);
    for (let i = 0; i < size * size; i++) rgba.set([120, 120, 90, 244], i * 4);
    colorEdgesFromFigure(rgba, size, size);
    const middle = ((size / 2) * size + size / 2) * 4;
    assert.deepEqual([...rgba.subarray(middle, middle + 4)], [120, 120, 90, 244]);
  });
});

describe("removeBackground", () => {
  it("край бурого персонажа на зелёном не темнее самого персонажа", async () => {
    // Бурый квадрат на фоне, обведённый полосой смеси бурого с фоном пополам.
    const BROWN = [150, 100, 70];
    const size = 12;
    const pixels = Buffer.alloc(size * size * 3);
    for (let y = 0; y < size; y++) {
      for (let x = 0; x < size; x++) {
        const ring = Math.max(Math.abs(x - 5.5), Math.abs(y - 5.5));
        const color = ring < 2 ? BROWN : ring < 3 ? BROWN.map((c, k) => (c + [0, 240, 0][k]) / 2) : [0, 240, 0];
        pixels.set(color.map(Math.round), (y * size + x) * 3);
      }
    }
    const png = await sharp(pixels, { raw: { width: size, height: size, channels: 3 } }).png().toBuffer();
    const { rgba } = await removeBackground(png, "green");
    const edge = (3 * size + 6) * 4;
    assert.ok(rgba[edge + 3] > 0 && rgba[edge + 3] < 255, `прозрачность края ${rgba[edge + 3]}`);
    assert.deepEqual([...rgba.subarray(edge, edge + 3)], BROWN);
  });
});

describe("keyPlate", () => {
  const SIZE = 120;
  // Фон — земля в мелкую клетку двух оттенков; кадр — тот же фон, на котором что-то нарисовано.
  function ground() {
    const rgb = Buffer.alloc(SIZE * SIZE * 3);
    for (let p = 0; p < SIZE * SIZE; p++) rgb.set((p % SIZE + Math.floor(p / SIZE)) % 2 ? [150, 100, 50] : [140, 95, 45], p * 3);
    return rgb;
  }
  function paint(rgb, left, top, side, color) {
    for (let y = top; y < top + side; y++) for (let x = left; x < left + side; x++) rgb.set(color, (y * SIZE + x) * 3);
  }
  const alpha = (rgba, x, y) => rgba[(y * SIZE + x) * 4 + 3];

  it("фон, перекрашенный видеомоделью целиком, прозрачен, а персонаж сплошной и без её сдвига цвета", () => {
    const plate = ground();
    const frame = Buffer.from(plate.map((value, i) => (i % 3 === 0 ? value - 40 : value)));
    paint(frame, 20, 20, 20, [200, 30, 30]);
    const rgba = keyPlate(frame, plate, SIZE, SIZE);
    assert.equal(alpha(rgba, 5, 5), 0);
    assert.equal(alpha(rgba, 30, 30), 255);
    assert.equal(rgba[(30 * SIZE + 30) * 4], 240);
  });

  it("мелкое отличие от фона — рябь сжатия, а не персонаж", () => {
    const plate = ground();
    const frame = Buffer.from(plate);
    paint(frame, 10, 10, 3, [255, 255, 255]);
    assert.equal(alpha(keyPlate(frame, plate, SIZE, SIZE), 11, 11), 0);
  });

  it("пятно вдали от персонажа снимается, а отрезанная от него кисть рядом остаётся", () => {
    const plate = ground();
    const frame = Buffer.from(plate);
    paint(frame, 70, 70, 25, [200, 30, 30]);
    paint(frame, 50, 70, 15, [230, 200, 190]);
    paint(frame, 0, 0, 15, [230, 200, 190]);
    const rgba = keyPlate(frame, plate, SIZE, SIZE);
    assert.equal(alpha(rgba, 56, 76), 255);
    assert.equal(alpha(rgba, 5, 5), 0);
  });

  it("дырка внутри персонажа, совпавшая с фоном, заливается", () => {
    const plate = ground();
    const frame = Buffer.from(plate);
    paint(frame, 15, 15, 30, [200, 30, 30]);
    for (let y = 25; y < 35; y++) for (let x = 25; x < 35; x++) frame.set(plate.subarray((y * SIZE + x) * 3, (y * SIZE + x) * 3 + 3), (y * SIZE + x) * 3);
    assert.equal(alpha(keyPlate(frame, plate, SIZE, SIZE), 30, 30), 255);
  });

  it("большая дырка внутри персонажа — настоящий фон, она остаётся прозрачной", () => {
    const plate = ground();
    const frame = Buffer.from(plate);
    paint(frame, 10, 10, 40, [200, 30, 30]);
    for (let y = 19; y < 41; y++) for (let x = 19; x < 41; x++) frame.set(plate.subarray((y * SIZE + x) * 3, (y * SIZE + x) * 3 + 3), (y * SIZE + x) * 3);
    assert.equal(alpha(keyPlate(frame, plate, SIZE, SIZE), 30, 30), 0);
  });

  it("узкий залив в контуре, совпавший с фоном, заливается", () => {
    const plate = ground();
    const frame = Buffer.from(plate);
    paint(frame, 15, 15, 30, [200, 30, 30]);
    for (let y = 15; y < 30; y++) for (let x = 29; x < 32; x++) frame.set(plate.subarray((y * SIZE + x) * 3, (y * SIZE + x) * 3 + 3), (y * SIZE + x) * 3);
    assert.equal(alpha(keyPlate(frame, plate, SIZE, SIZE), 30, 20), 255);
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
  function frame(extra, keepColor = false) {
    const pixels = [...Array(10).fill(BACKGROUND), ...Array(10).fill(GRASS), ...extra.map(([color]) => color)];
    const luma = [...Array(10).fill(102), ...Array(10).fill(30), ...extra.map(([, level]) => level)];
    return keyVideoFrame(Buffer.from(pixels.flat()), Uint8Array.from(luma), "magenta", keepColor);
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

  it("с keepColor сплошной красный сарафан остаётся красным", () => {
    // Без keepColor у красного снимается составляющая вдоль розового фона, и сарафан (188, 44, 2) выходит охристым.
    const sarafan = [188, 44, 2];
    const color = colorAt(frame([[sarafan, 82]], true), 20);
    color.forEach((channel, c) => assert.ok(Math.abs(channel - sarafan[c]) <= 2, `сарафан ${color}`));
  });

  it("с keepColor золотое сияние с розовой примесью теряет розовое", () => {
    // Сияние почти непрозрачно, и примесь фона в нём видна только по синему выше зелёного.
    const [, green, blue] = colorAt(frame([[[250, 190, 215], 211]], true), 20);
    assert.ok(blue < green, `сияние ${[green, blue]}`);
  });

  it("с keepColor полупрозрачный край из травы и фона всё равно теряет розовое", () => {
    const mixed = [133, 68, 128];
    const [, green, blue] = colorAt(frame([[mixed, 66]], true), 20);
    assert.ok(blue - green < mixed[2] - mixed[1] - 20, `край ${[green, blue]}`);
  });

  it("зелёная трава своего цвета не меняет", () => {
    const color = colorAt(frame([]), 10);
    color.forEach((channel, c) => assert.ok(Math.abs(channel - GRASS[c]) <= 1, `трава ${color}`));
  });

  it("на зелёном тень у тёмного персонажа, темнее фона по яркости, остаётся прозрачной", () => {
    // Десять точек фона яркостью 141, десять точек тёмного персонажа яркостью 64 и тень: по цвету фон, по яркости темнее.
    const pixels = [...Array(10).fill([0, 240, 0]), ...Array(10).fill([80, 60, 50]), [25, 207, 12]];
    const luma = [...Array(10).fill(141), ...Array(10).fill(64), 130];
    const rgba = keyVideoFrame(Buffer.from(pixels.flat()), Uint8Array.from(luma), "green");
    assert.equal(alphaAt(rgba, 20), 0);
  });

  it("на зелёном тонкая тёмная прядь, у которой видео смешало цвет с фоном пополам, видна хотя бы наполовину", () => {
    // Прядь (50, 35, 30) с яркостью пряди и цветностью наполовину от фона (0, 240, 0) — точка (0, 87, 0).
    const pixels = [...Array(10).fill([0, 240, 0]), ...Array(10).fill([80, 60, 50]), [0, 87, 0]];
    const luma = [...Array(10).fill(141), ...Array(10).fill(64), 39];
    const alpha = alphaAt(keyVideoFrame(Buffer.from(pixels.flat()), Uint8Array.from(luma), "green"), 20);
    assert.ok(alpha >= 128 && alpha < 255, `прозрачность ${alpha}`);
  });

  it("у светлого куста, по яркости равного фону, рябь фона остаётся прозрачной", () => {
    // Листва (120, 140, 40) яркостью 104 на розовом фоне яркостью 102: ключ по яркости сделал бы точку фона яркостью 104
    // сплошной, хотя по цвету она чистый фон.
    const pixels = [...Array(10).fill(BACKGROUND), ...Array(10).fill([120, 140, 40]), BACKGROUND];
    const luma = [...Array(10).fill(102), ...Array(10).fill(104), 104];
    const rgba = keyVideoFrame(Buffer.from(pixels.flat()), Uint8Array.from(luma), "magenta");
    assert.equal(alphaAt(rgba, 20), 0);
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

describe("reversePath", () => {
  it("ставит -reverse перед расширением рядом с выходом", () => {
    assert.equal(reversePath(join("games", "x", "ilya_greet.mp4")), join("games", "x", "ilya_greet-reverse.mp4"));
    assert.equal(reversePath("clip.MP4"), "clip-reverse.mp4");
  });
});

describe("makeVideo с reverse", () => {
  // Два прямоугольника на розовом фоне: левый в первую секунду, правый во вторую — 48 кадров по 24 в секунду.
  function makeClip(dir) {
    const clip = join(dir, "clip.mp4");
    const make = spawnSync(ffmpegPath, [
      "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=c=0xE22DE1:s=96x48:r=24:d=2",
      "-vf", "drawbox=x=8:y=15:w=20:h=12:color=0x1E3C14:t=fill:enable='lt(n,24)',drawbox=x=68:y=15:w=20:h=12:color=0x1E3C14:t=fill:enable='gte(n,24)'",
      "-c:v", "libx264", "-pix_fmt", "yuv420p", clip,
    ], { encoding: "utf8" });
    assert.equal(make.status, 0, make.stderr);
    return clip;
  }

  function frameInfos(file) {
    const run = spawnSync(ffmpegPath, ["-hide_banner", "-i", file, "-vf", "showinfo", "-f", "null", "-"], { encoding: "utf8" });
    assert.equal(run.status, 0, run.stderr);
    return [...run.stderr.matchAll(/n:\s*(\d+).*iskey:(\d)/g)].map((match) => ({ number: Number(match[1]), isKey: match[2] === "1" }));
  }

  async function maskAt(file, frame, side) {
    const png = join(file + `.${frame}.png`);
    const decode = spawnSync(ffmpegPath, ["-hide_banner", "-loglevel", "error", "-y", "-i", file, "-vf", `select='eq(n,${frame})'`, "-frames:v", "1", png], { encoding: "utf8" });
    assert.equal(decode.status, 0, decode.stderr);
    const { data, info } = await sharp(png).removeAlpha().raw().toBuffer({ resolveWithObject: true });
    const half = info.height / 2;
    const y = half + Math.floor(half / 2);
    return data[(y * info.width + (side === "left" ? Math.floor(info.width * 0.12) : Math.floor(info.width * 0.88))) * 3];
  }

  it("пишет рядом <выход>-reverse.mp4 с теми же кадрами задом наперёд и ключевым кадром не реже полсекунды", async () => {
    const dir = mkdtempSync(join(tmpdir(), "kuznya-reverse-test-"));
    try {
      const output = join(dir, "out.mp4");
      await makeVideo(makeClip(dir), output, { key: "magenta", range: parseRange("1-48"), reverse: true });
      const reversed = reversePath(output);

      assert.equal(frameInfos(output).length, 48);
      assert.equal(frameInfos(reversed).length, 48);
      assert.ok(await maskAt(output, 0, "left") > 200 && await maskAt(output, 0, "right") < 60, "обычный: первый кадр слева");
      assert.ok(await maskAt(reversed, 0, "right") > 200 && await maskAt(reversed, 0, "left") < 60, "обратный: первый кадр справа");
      assert.ok(await maskAt(reversed, 47, "left") > 200, "обратный: последний кадр слева");
      for (const file of [output, reversed]) {
        const keys = [...frameInfos(file).filter((info) => info.isKey).map((info) => info.number), 48];
        keys.forEach((key, i) => i > 0 && assert.ok(key - keys[i - 1] <= 12, `${file}: ключевые кадры ${keys}`));
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("без reverse обратного файла не пишет", async () => {
    const dir = mkdtempSync(join(tmpdir(), "kuznya-reverse-test-"));
    try {
      const output = join(dir, "out.mp4");
      await makeVideo(makeClip(dir), output, { key: "magenta", range: parseRange("1-48") });
      assert.equal(existsSync(reversePath(output)), false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("--reverse у выхода не .mp4 — ошибка", () => {
    const run = spawnSync(process.execPath, [fileURLToPath(new URL("./sheet.mjs", import.meta.url)), "clip.mp4", "sheet.png", "--reverse"], { encoding: "utf8" });
    assert.equal(run.status, 1);
    assert.match(run.stderr, /--reverse — только с выходом \.mp4/);
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
