import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { buildCovers, buildGrid } from "./build.mjs";
import { MASK_PER_CELL } from "./covers.mjs";
import { terrainText } from "./files.mjs";

const WATER = { level: -2.2, color: "#3f7fd0" };
const MOUNTAINS = { op: "range", name: "горы", foot: [[-2, 12], [42, 12]], side: "left", height: 12, depth: 6, roughness: 0 };

function plan(operations, layers, overrides = {}) {
  return { size: [40, 30], seed: 7, terrain: "terrain.json", water: WATER, operations, covers: [{ material: "grass" }, ...layers], ...overrides };
}

/** Маски описания по имени материала; `at(x, y)` — точка маски в месте сцены. */
function masksOf(description) {
  const { masks } = buildCovers(description, buildGrid(description));
  return new Map(
    masks.map((mask) => [mask.material, { ...mask, at: (x, y) => mask.pixels[Math.floor(y * MASK_PER_CELL) * mask.width + Math.floor(x * MASK_PER_CELL)] }]),
  );
}

describe("правила масок", () => {
  it("patches: пятна по всей сцене занимают около своей доли, край пятна плавный", () => {
    const lush = masksOf(plan([], [{ material: "lush", rule: "patches", share: 0.35, wavelength: 8, soft: 0.2 }])).get("lush");
    const covered = lush.pixels.filter((value) => value > 127).length / lush.pixels.length;
    assert.ok(covered > 0.25 && covered < 0.45, `пятна заняли ${covered} сцены`);
    assert.ok(lush.pixels.some((value) => value > 20 && value < 235), "на краю пятна есть промежуточные значения");
  });

  it("pebbles: галька на дне у воды, выше уровня воды плюс один нет", () => {
    const channel = { op: "channel", name: "ручей", line: [[0, 15], [40, 15]], width: 2, bottom: -3, bank: 1 };
    const pebbles = masksOf(plan([channel], [{ material: "pebbles", rule: "pebbles" }])).get("pebbles");
    assert.equal(pebbles.at(20, 15), 255);
    assert.equal(pebbles.at(20, 5), 0);
  });

  it("patches: область вне сцены — ошибка, а не пустая маска", () => {
    const description = plan([], [{ material: "lush", rule: "patches", share: 0.3, wavelength: 8, area: [[50, 50], [60, 50], [60, 60]] }]);
    assert.throws(() => buildCovers(description, buildGrid(description)), /покрытие «lush»: область пятен не задевает сцену/);
  });

  it("pebbles: без воды в описании ошибка", () => {
    assert.throws(() => buildGrid(plan([], [{ material: "pebbles", rule: "pebbles" }], { water: undefined })), /«pebbles»: галька лежит у воды/);
  });

  it("path: тропа под линией целиком, в стороне нет, край мягкий", () => {
    const path = masksOf(plan([], [{ material: "path", rule: "path", lines: [{ width: 2, points: [[2, 10], [38, 10]] }] }])).get("path");
    assert.equal(path.at(20, 10), 255);
    assert.equal(path.at(20, 10.5), 255);
    assert.equal(path.at(20, 13), 0);
    assert.equal(path.at(0.1, 10), 0, "тропа кончается там, где кончается линия");
    const across = Array.from({ length: 12 }, (_, i) => path.at(20, 11 + i / 8));
    assert.ok(across.some((value) => value > 0 && value < 255), "на краю есть промежуточные значения");
  });

  it("earth: площадка целиком, вытоптанная полоса только с «worn», проплешины только внутри области и около четверти её площади", () => {
    const pad = { op: "pad", name: "площадка", area: [[26, 15], [34, 15], [34, 20], [26, 20]], height: 0, rim: 1 };
    const track = { material: "path", rule: "path", lines: [{ width: 2.5, points: [[2, 25], [38, 25]] }] };
    const area = [[2, 2], [22, 2], [22, 12], [2, 12]];
    const description = plan(
      [pad],
      [{ material: "earth", rule: "earth", pad: "площадка", worn: 1, patches: { area, share: 0.25, wavelength: 6 } }, track],
    );
    const earth = masksOf(description).get("earth");
    assert.equal(earth.at(30, 17.5), 255, "площадка");
    assert.equal(earth.at(20, 25), 255, "под тропой и у её края");
    assert.equal(earth.at(20, 29), 0, "дальше полосы");
    const unworn = masksOf(plan([pad], [{ material: "earth", rule: "earth", pad: "площадка" }, track])).get("earth");
    assert.equal(unworn.at(20, 25), 0, "без «worn» вдоль тропы земли нет");
    assert.equal(earth.at(35, 8), 0, "проплешин вне области нет");
    let inside = 0;
    let covered = 0;
    for (let y = 2; y < 12; y += 0.25) {
      for (let x = 2; x < 22; x += 0.25) {
        inside += 1;
        if (earth.at(x, y) > 127) covered += 1;
      }
    }
    assert.ok(covered / inside > 0.12 && covered / inside < 0.32, `проплешины заняли ${covered / inside} области`);
  });
});

describe("покрытия описания", () => {
  const layers = [
    { material: "earth", rule: "earth" },
    { material: "path", rule: "path", lines: [{ width: 2, points: [[2, 10], [38, 10]] }] },
    { material: "rock", slope: 34 },
  ];

  it("слои — по порядку описания, маска у слоёв с правилом, у первого и у слоя со «slope» без правила её нет; маски по четыре точки на клетку", () => {
    const description = plan([MOUNTAINS], layers);
    const { layers: read, masks } = buildCovers(description, buildGrid(description));
    assert.deepEqual(
      read.map(({ material, mask, slope }) => [material, mask, slope]),
      [
        ["grass", undefined, undefined],
        ["earth", "terrain/earth.png", undefined],
        ["path", "terrain/path.png", undefined],
        ["rock", undefined, 34],
      ],
    );
    assert.deepEqual(
      masks.map(({ material, width, height }) => [material, width, height]),
      [
        ["earth", 160, 120],
        ["path", 160, 120],
      ],
    );
  });

  it("слой со «slope» и правилом получает и маску, и порог; порог переходит в слой как есть", () => {
    const description = plan([], [{ material: "lush", rule: "patches", share: 0.3, wavelength: 8, slope: 33.5 }, { material: "rock", slope: 0 }, { material: "ice", slope: 90 }]);
    const { layers: read, masks } = buildCovers(description, buildGrid(description));
    assert.deepEqual(
      read.map(({ material, mask, slope }) => [material, mask, slope]),
      [
        ["grass", undefined, undefined],
        ["lush", "terrain/lush.png", 33.5],
        ["rock", undefined, 0],
        ["ice", undefined, 90],
      ],
    );
    assert.deepEqual(masks.map(({ material }) => material), ["lush"]);
  });

  it("одно описание — одни и те же маски, другое зерно — другие края", () => {
    const build = (description) => buildCovers(description, buildGrid(description)).masks.map(({ pixels }) => [...pixels]);
    assert.deepEqual(build(plan([MOUNTAINS], layers)), build(plan([MOUNTAINS], layers)));
    assert.notDeepEqual(build(plan([MOUNTAINS], layers)), build(plan([MOUNTAINS], layers, { seed: 8 })));
  });

  it("без covers слоёв и масок нет, файл рельефа без раздела", () => {
    const description = plan([MOUNTAINS], [], { covers: undefined });
    const { layers: read, masks } = buildCovers(description, buildGrid(description));
    assert.equal(read, undefined);
    assert.deepEqual(masks, []);
    assert.ok(!terrainText(buildGrid(description), WATER, read).includes("covers"));
  });

  it("файл рельефа: covers после water, по слою на строку, затем heights; порог крутизны — перед маской", () => {
    const description = plan([], [{ material: "moss", slope: 34 }, { material: "lush", rule: "patches", share: 0.3, wavelength: 8, slope: 20 }]);
    const { layers: read } = buildCovers(description, buildGrid(description));
    const lines = terrainText(buildGrid(description), WATER, read).split("\n");
    assert.deepEqual(lines.slice(0, 8), [
      "{",
      '  "water": { "level": -2.2, "color": "#3f7fd0" },',
      '  "covers": [',
      '    { "material": "grass" },',
      '    { "material": "moss", "slope": 34 },',
      '    { "material": "lush", "slope": 20, "mask": "terrain/lush.png" }',
      "  ],",
      '  "heights": [',
    ]);
  });
});

describe("ошибки покрытий", () => {
  const path = { material: "path", rule: "path", lines: [{ width: 2, points: [[2, 10], [38, 10]] }] };
  const cases = [
    [plan([], [], { covers: [] }), "covers — список из 1–8 слоёв"],
    [plan([], [], { covers: "grass" }), "covers — список из 1–8 слоёв"],
    [plan([], Array.from({ length: 8 }, (_, i) => ({ material: `m${i}`, rule: "pebbles" }))), "covers — список из 1–8 слоёв"],
    [plan([], [], { covers: ["grass"] }), "покрытие 1: слой — объект JSON"],
    [plan([], [], { covers: [{ material: "gr ass" }] }), "покрытие 1 «gr ass»: «material» — имя из латинских букв"],
    [plan([], [], { covers: [{ material: "grass", mask: "terrain/grass.png" }] }), "покрытие 1 «grass»: неизвестный ключ «mask»"],
    [plan([], [{ material: "grass", rule: "pebbles" }]), "покрытие 2 «grass»: этот материал уже есть"],
    [plan([], [{ material: "rock" }]), "покрытие 2 «rock»: нужно «rule» (одно из: patches, earth, pebbles, path) или «slope»"],
    [plan([], [{ material: "rock", rule: "cliff" }]), "«rule» — одно из"],
    [plan([], [{ material: "rock", rule: "slope", from: 30, full: 38 }]), "покрытие 2 «rock»: «rule» — одно из: patches, earth, pebbles, path"],
    [plan([], [{ material: "rock", rule: "scree", range: "горы" }]), "«rule» — одно из: patches, earth, pebbles, path"],
    [plan([], [], { covers: [{ material: "grass", slope: 30 }] }), "покрытие 1 «grass»: неизвестный ключ «slope»"],
    [plan([], [{ material: "rock", slope: -1 }]), "покрытие 2 «rock»: «slope» — градусы крутизны, число от 0 до 90"],
    [plan([], [{ material: "rock", slope: 91 }]), "«slope» — градусы крутизны, число от 0 до 90"],
    [plan([], [{ material: "rock", slope: "30" }]), "«slope» — градусы крутизны, число от 0 до 90"],
    [plan([], [{ material: "rock", rule: "pebbles", slope: 120 }]), "«slope» — градусы крутизны, число от 0 до 90"],
    [plan([], [{ material: "rock", slope: 30, mask: "terrain/rock.png" }]), "покрытие 2 «rock»: неизвестный ключ «mask»"],
    [plan([], [{ material: "rock", slope: 30, from: 20 }]), "неизвестный ключ «from»"],
    [plan([], [{ material: "rock", rule: "pebbles", mask: "x.png" }]), "покрытие 2 «rock»: неизвестный ключ «mask»"],
    [plan([], [{ material: "path", rule: "path" }]), "покрытие 2 «path»: нет обязательного ключа «lines»"],
    [plan([], [{ ...path, lines: [] }]), "«lines» — непустой список троп"],
    [plan([], [{ ...path, lines: [{ width: 0, points: [[0, 0], [5, 5]] }] }]), "«lines»[1]: «width» — число больше 0"],
    [plan([], [{ ...path, lines: [{ width: 1, points: [[0, 0]] }] }]), "«lines»[1].points» — список не меньше 2 точек"],
    [plan([], [{ ...path, lines: [{ width: 1, points: [[0, 0], [5, 5]], color: 1 }] }]), "«lines»[1]: неизвестный ключ «color»"],
    [plan([], [{ material: "lush", rule: "patches", wavelength: 8 }]), "нет обязательного ключа «share»"],
    [plan([], [{ material: "lush", rule: "patches", share: 0.3, wavelength: 8, soft: 0 }]), "«soft» — число больше 0"],
    [plan([], [{ material: "earth", rule: "earth", worn: 0.2 }]), "«worn» — число больше 0.2"],
    [plan([], [{ material: "earth", rule: "earth", pad: "нет такой" }]), "«pad» — имя операции «pad» из описания"],
    [plan([], [{ material: "earth", rule: "earth", patches: { share: 0.25 } }]), "«patches»: нет обязательного ключа «area»"],
    [plan([], [{ material: "earth", rule: "earth", patches: { area: [[0, 0], [9, 0], [9, 9]], share: 1 } }]), "«share» — число больше 0 и меньше 1"],
    [plan([], [{ material: "earth", rule: "earth", patches: { area: [[0, 0], [9, 0]] } }]), "«patches.area» — список не меньше 3 точек"],
  ];
  for (const [description, message] of cases) {
    it(message, () => assert.throws(() => buildGrid(description), (error) => error.message.includes(message)));
  }
});
