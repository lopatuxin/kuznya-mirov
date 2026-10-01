import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { DEFAULT_BASE, DEFAULT_FADE, DEFAULT_POINTS, readCutList } from "./list.mjs";

const CUT = { name: "peak", place: "Белуха", lat: 49.8, lon: 86.6, km: [9, 7] };

function list(cut = {}, overrides = {}) {
  return { out: "stamps", stamps: [{ ...CUT, ...cut }], ...overrides };
}

describe("readCutList", () => {
  it("вырез получает значения по умолчанию, свои остаются", () => {
    const { out, cuts } = readCutList(list({}, { stamps: [CUT, { ...CUT, name: "other", points: 64, base: 0.2, fade: 0.4 }] }));
    assert.equal(out, "stamps");
    assert.deepEqual(cuts[0], { ...CUT, points: DEFAULT_POINTS, base: DEFAULT_BASE, fade: DEFAULT_FADE });
    assert.deepEqual([cuts[1].points, cuts[1].base, cuts[1].fade], [64, 0.2, 0.4]);
    assert.deepEqual([DEFAULT_POINTS, DEFAULT_BASE, DEFAULT_FADE], [192, 0.1, 0.55]);
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
  ];
  for (const [input, message] of cases) {
    it(message, () => assert.throws(() => readCutList(input), (error) => error.message.includes(message)));
  }
});
