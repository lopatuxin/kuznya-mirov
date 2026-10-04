import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { DEFAULT_BASE, DEFAULT_FADE, DEFAULT_ZOOM } from "./list.mjs";
import { cutStamp, stampText } from "./stamp.mjs";
import { TILE_SIZE } from "./tiles.mjs";

const world = (zoom) => TILE_SIZE * 2 ** zoom;
const METERS_PER_PIXEL_AT_ZERO_LATITUDE = (156543.03392804097 * 256) / world(DEFAULT_ZOOM);

function cut(overrides = {}) {
  return { name: "peak", place: "тест", lat: 50, lon: 87, km: [8, 6], points: 40, base: DEFAULT_BASE, fade: DEFAULT_FADE, zoom: DEFAULT_ZOOM, invert: false, ...overrides };
}

/** Читатель плиток для поля `metersAt(x, y)` по пикселям мира; список запрошенных плиток `[x, y, zoom]` — в `asked`. */
function fieldReader(metersAt) {
  const asked = [];
  const read = async (tileX, tileY, zoom) => {
    asked.push([tileX, tileY, zoom]);
    const heights = new Float64Array(TILE_SIZE * TILE_SIZE);
    for (let y = 0; y < TILE_SIZE; y++) {
      for (let x = 0; x < TILE_SIZE; x++) heights[y * TILE_SIZE + x] = metersAt(tileX * TILE_SIZE + x, tileY * TILE_SIZE + y);
    }
    return heights;
  };
  return { read, asked };
}

function worldPixel(lat, lon, zoom = DEFAULT_ZOOM) {
  return { x: ((lon + 180) / 360) * world(zoom), y: ((1 - Math.asinh(Math.tan((lat * Math.PI) / 180)) / Math.PI) / 2) * world(zoom) };
}

/** Конус высотой 1000 м и радиусом `radius` точек с вершиной в `peak`. */
function cone(peak, radius) {
  return (x, y) => Math.max(0, 1000 * (1 - Math.hypot(x - peak.x, y - peak.y) / radius));
}

function cellOfPeak({ heights }) {
  let best = [0, 0];
  heights.forEach((row, r) => row.forEach((value, c) => value > heights[best[0]][best[1]] && (best = [r, c])));
  return best;
}

describe("cutStamp", () => {
  const center = worldPixel(50, 87);

  it("размер сетки: длинная сторона — points точек, короткая — по пропорции", async () => {
    const { read } = fieldReader(cone(center, 600));
    const wide = await cutStamp(cut({ km: [8, 6] }), read);
    assert.deepEqual([wide.heights[0].length, wide.heights.length], [40, 30]);
    const tall = await cutStamp(cut({ km: [3, 9] }), read);
    assert.deepEqual([tall.heights[0].length, tall.heights.length], [13, 40]);
  });

  it("наибольшая высота — 1, числа до тысячных, края выреза — 0", async () => {
    const { read } = fieldReader(cone(center, 500));
    const { heights } = await cutStamp(cut(), read);
    assert.equal(Math.max(...heights.flat()), 1);
    assert.ok(heights.flat().every((value) => value >= 0 && value <= 1 && Math.round(value * 1000) / 1000 === value));
    assert.ok(heights[0].every((value) => value === 0) && heights.at(-1).every((value) => value === 0), "северный и южный края");
    assert.ok(heights.every((row) => row[0] === 0 && row.at(-1) === 0), "западный и восточный края");
  });

  it("вершина конуса в середине выреза — наибольшая точка в середине штампа", async () => {
    const { read } = fieldReader(cone(center, 500));
    const [row, col] = cellOfPeak(await cutStamp(cut({ points: 41, km: [8, 6] }), read));
    assert.deepEqual([row, col], [15, 20]);
  });

  it("первая строка — север, первое число строки — запад; километры — настоящие, с поправкой на широту", async () => {
    const east = 2000 / (METERS_PER_PIXEL_AT_ZERO_LATITUDE * Math.cos((50 * Math.PI) / 180));
    const north = 1500 / (METERS_PER_PIXEL_AT_ZERO_LATITUDE * Math.cos((50 * Math.PI) / 180));
    // Вершина в 2 км к востоку и 1,5 км к северу от середины выреза 8 × 6 км: на четверть ширины и глубины.
    const { read } = fieldReader(cone({ x: center.x + east, y: center.y - north }, 300));
    const stamp = await cutStamp(cut({ points: 81, km: [8, 6], fade: 0.3 }), read);
    const [row, col] = cellOfPeak(stamp);
    assert.equal(stamp.heights.length, 61);
    assert.ok(Math.abs(col - 60) <= 1, `столбец ${col}, ждали 60 из 81`);
    assert.ok(Math.abs(row - 15) <= 1, `строка ${row}, ждали 15 из 61`);
  });

  it("на широте 60° вырез охватывает вдвое больше пикселей, чем на экваторе: плиток запрошено вдвое больше", async () => {
    const ramp = (x) => x * 0.01;
    const equator = fieldReader(ramp);
    await cutStamp(cut({ lat: 0, km: [20, 10], points: 40 }), equator.read);
    const north = fieldReader(ramp);
    await cutStamp(cut({ lat: 60, km: [20, 10], points: 40 }), north.read);
    const columns = (asked) => new Set(asked.map(([x]) => x)).size;
    assert.ok(columns(equator.asked) >= 5 && columns(equator.asked) <= 6, `на экваторе ${columns(equator.asked)} колонок`);
    assert.ok(columns(north.asked) >= 9 && columns(north.asked) <= 10, `на 60° ${columns(north.asked)} колонок`);
  });

  it("точка штампа — среднее точек плиток: шахматная рябь на склоне не попадает в штамп", async () => {
    const { read } = fieldReader((x, y) => cone(center, 500)(x, y) + ((x + y) % 2 === 0 ? 100 : 0));
    const { heights } = await cutStamp(cut({ points: 30, fade: 0.1 }), read);
    const row = heights[10];
    for (let col = 5; col <= 10; col++) {
      const bend = Math.abs(row[col - 1] - 2 * row[col] + row[col + 1]);
      assert.ok(bend < 0.015, `перегиб в столбце ${col}: ${bend}`);
    }
  });

  it("основание: что ниже доли base точек — 0, ровное поле с бугром оставляет только бугор", async () => {
    const bump = (x, y) => 300 + cone(center, 250)(x, y);
    const { read } = fieldReader(bump);
    const { heights } = await cutStamp(cut({ base: 0.5, fade: 0.9 }), read);
    const zeros = heights.flat().filter((value) => value === 0).length / heights.flat().length;
    assert.ok(zeros >= 0.5, `нулей ${zeros}`);
    assert.equal(heights[15][20], 1);
  });

  it("гашение края: у края прямоугольника 0, чем меньше fade, тем круче стена, середина не меняется", async () => {
    const { read } = fieldReader((x, y) => 500 + cone(center, 700)(x, y));
    const soft = (await cutStamp(cut({ fade: 1, base: 0 }), read)).heights;
    const sharp = (await cutStamp(cut({ fade: 0.2, base: 0 }), read)).heights;
    assert.ok(sharp[3][20] > soft[3][20] + 0.3);
    assert.deepEqual([soft[0][20], sharp[0][20], soft[15][0], sharp[15][0]], [0, 0, 0, 0]);
    assert.deepEqual([soft[15][20], sharp[15][20]], [1, 1]);
  });

  it("плоский вырез — ошибка, а не NaN в файле", async () => {
    await assert.rejects(cutStamp(cut(), fieldReader(() => 120).read), /высот не осталось/);
  });

  it("одни и те же плитки и вырез — тот же файл байт в байт", async () => {
    const field = (x, y) => 800 + 400 * Math.sin(x / 37) * Math.cos(y / 53) + cone(center, 400)(x, y);
    const first = stampText(await cutStamp(cut(), fieldReader(field).read));
    const second = stampText(await cutStamp(cut(), fieldReader(field).read));
    assert.equal(first, second);
  });

  it("масштаб выреза уходит в читатель плиток; на масштабе 15 тот же вырез — вчетверо больше плиток по краю, чем на 13", async () => {
    const ramp = (x) => x * 0.01;
    const columns = async (zoom) => {
      const reader = fieldReader(ramp);
      await cutStamp(cut({ zoom, km: [20, 10], points: 40 }), reader.read);
      assert.ok(reader.asked.every(([, , asked]) => asked === zoom));
      return new Set(reader.asked.map(([x]) => x)).size;
    };
    const coarse = await columns(13);
    const fine = await columns(15);
    assert.ok(fine >= 4 * coarse - 4 && fine <= 4 * coarse, `на 13 — ${coarse} колонок, на 15 — ${fine}`);
  });

  it("масштаб 15: вершина конуса в середине выреза — наибольшая точка в середине штампа", async () => {
    const middle = worldPixel(50, 87, 15);
    const [row, col] = cellOfPeak(await cutStamp(cut({ zoom: 15, points: 41, km: [8, 6] }), fieldReader(cone(middle, 2000)).read));
    assert.deepEqual([row, col], [15, 20]);
  });

  it("invert: впадина становится холмом — самое низкое место 1, край 0, и это тот же штамп, что у конуса того же размера", async () => {
    const bowl = (x, y) => 1000 - cone(center, 500)(x, y);
    const inverted = await cutStamp(cut({ invert: true, base: 0, fade: 0.4 }), fieldReader(bowl).read);
    const hill = await cutStamp(cut({ base: 0, fade: 0.4 }), fieldReader(cone(center, 500)).read);
    assert.equal(inverted.heights[15][20], 1);
    assert.ok(inverted.heights[0].every((value) => value === 0) && inverted.heights.every((row) => row[0] === 0 && row.at(-1) === 0));
    inverted.heights.forEach((row, r) => row.forEach((value, c) => assert.ok(Math.abs(value - hill.heights[r][c]) <= 0.002, `строка ${r}, столбец ${c}: ${value} против ${hill.heights[r][c]}`)));
  });

  it("invert: холм без переворота и с ним — разные штампы, а без invert файл тот же, что с invert: false", async () => {
    const field = cone(center, 500);
    const plain = stampText(await cutStamp(cut(), fieldReader(field).read));
    const { invert, ...withoutInvert } = cut();
    assert.equal(invert, false);
    assert.equal(stampText(await cutStamp(withoutInvert, fieldReader(field).read)), plain);
    assert.notEqual(stampText(await cutStamp(cut({ invert: true }), fieldReader(field).read)), plain);
  });

  it("invert на ровном месте — та же ошибка, что без invert", async () => {
    await assert.rejects(cutStamp(cut({ invert: true }), fieldReader(() => 120).read), /высот не осталось/);
  });

  it("ошибка чтения плитки доходит до вызывающего", async () => {
    await assert.rejects(
      cutStamp(cut(), async () => {
        throw new Error("плитка не скачалась");
      }),
      /плитка не скачалась/,
    );
  });
});

describe("stampText", () => {
  it("первая строка — «{ \"heights\": [», по строке штампа на строку файла, конец — «] }»", () => {
    assert.equal(stampText({ heights: [[0, 0.5, 1], [0.25, 0.125, 0]] }), '{ "heights": [\n  [0, 0.5, 1],\n  [0.25, 0.125, 0]\n] }\n');
  });

  it("-0 пишется 0", () => {
    assert.ok(!stampText({ heights: [[-0, 1], [0, 0]] }).includes("-0"));
  });
});
