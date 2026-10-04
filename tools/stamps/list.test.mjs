import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { DEFAULT_BASE, DEFAULT_FADE, DEFAULT_POINTS, DEFAULT_ZOOM, readCutList } from "./list.mjs";

const CUT = { name: "peak", place: "Белуха", lat: 49.8, lon: 86.6, km: [9, 7] };

function list(cut = {}, overrides = {}) {
  return { out: "stamps", stamps: [{ ...CUT, ...cut }], ...overrides };
}

describe("readCutList", () => {
  it("вырез получает значения по умолчанию, свои остаются", () => {
    const { out, cuts } = readCutList(list({}, { stamps: [CUT, { ...CUT, name: "other", points: 64, base: 0.2, fade: 0.4 }] }));
    assert.equal(out, "stamps");
    assert.deepEqual(cuts[0], { ...CUT, points: DEFAULT_POINTS, base: DEFAULT_BASE, fade: DEFAULT_FADE, zoom: DEFAULT_ZOOM, invert: false });
    assert.deepEqual([cuts[1].points, cuts[1].base, cuts[1].fade], [64, 0.2, 0.4]);
    assert.deepEqual([DEFAULT_POINTS, DEFAULT_BASE, DEFAULT_FADE, DEFAULT_ZOOM], [192, 0.1, 0.55, 13]);
  });

  it("zoom и invert: свои значения остаются, zoom — от 10 до 15 включительно", () => {
    const { cuts } = readCutList(list({}, { stamps: [{ ...CUT, zoom: 10 }, { ...CUT, name: "other", zoom: 15, invert: true }] }));
    assert.deepEqual([cuts[0].zoom, cuts[0].invert, cuts[1].zoom, cuts[1].invert], [10, false, 15, true]);
  });

  const cases = [
    [null, "список вырезов должен быть объектом JSON"],
    [list({}, { extra: 1 }), "неизвестный ключ списка «extra»"],
    [list({}, { out: undefined }), "out — путь папки штампов"],
    [list({}, { stamps: [] }), "stamps — непустой список вырезов"],
    [list({}, { stamps: "peak" }), "stamps — непустой список вырезов"],
    [list({}, { stamps: [CUT, { ...CUT, place: "ещё" }] }), "вырез 2 «peak»: это имя уже есть"],
    [list({ place: undefined }), "вырез 1 «peak»: нет обязательного ключа «place»"],
    [list({ km: undefined }), "нет обязательного ключа «km»"],
    [list({ name: undefined }), "вырез 1: нет обязательного ключа «name»"],
    [list({ size: 3 }), "вырез 1 «peak»: неизвестный ключ «size»"],
    [list({ name: "a b" }), "«name» — имя из латинских букв"],
    [list({ lat: 90 }), "«lat» — широта"],
    [list({ lat: "49" }), "«lat» — широта"],
    [list({ lon: 181 }), "«lon» — долгота"],
    [list({ km: [9] }), "«km» — два числа"],
    [list({ km: [0, 5] }), "«km» — два числа"],
    [list({ km: [5, 101] }), "«km» — два числа"],
    [list({ points: 1 }), "«points» — целое число от 2"],
    [list({ points: 2.5 }), "«points» — целое число от 2"],
    [list({ base: 1 }), "«base» — число от 0 до 1"],
    [list({ base: -0.1 }), "«base» — число от 0 до 1"],
    [list({ fade: 0 }), "«fade» — число больше 0"],
    [list({ fade: 1.5 }), "«fade» — число больше 0"],
    [list({ zoom: 9 }), "вырез 1 «peak»: «zoom» — целое число от 10 до 15"],
    [list({ zoom: 16 }), "«zoom» — целое число от 10 до 15"],
    [list({ zoom: 13.5 }), "«zoom» — целое число от 10 до 15"],
    [list({ zoom: "13" }), "«zoom» — целое число от 10 до 15"],
    [list({ invert: 1 }), "вырез 1 «peak»: «invert» — true или false"],
    [list({ invert: "true" }), "«invert» — true или false"],
    [list({ invert: null }), "«invert» — true или false"],
  ];
  for (const [input, message] of cases) {
    it(message, () => assert.throws(() => readCutList(input), (error) => error.message.includes(message)));
  }
});
