import { describe, expect, it } from "vitest";
import { buildGrassGround, buildScene, buildWallStripObjects, mergeVillageCredits, validatePiece } from "./buildRpgVillageScene.mjs";
import { GROUND_LAYER, OBSTACLE_LAYER, PIECES } from "./rpg-village/catalog.mjs";
import { ENEMIES, HERO_START, SCENE_HEIGHT, SCENE_WIDTH } from "./rpg-village/layout.mjs";
import { reachableCells, isReachable } from "./rpg-village/reachability.mjs";

const TILE = 96;

describe("buildWallStripObjects — требования 2, 13, 14", () => {
  // Кусок в 3 столбца, низ каждого столбца снижается на 40 точек — имитирует измеренную диагональ
  // без чтения настоящей картинки.
  const meta = { bottoms: [200, 160, 120], frameCount: 3, height: 240 };

  it("полоска — кадр с низом на измеренном низу своего столбца (без отражения, два вправо на одну вверх)", () => {
    const objects = buildWallStripObjects({ name: "w", x: 5, y: 10, flip: false }, "wall_log", meta);
    const strips = objects.filter((o) => o.name.startsWith("w_strip_"));
    expect(strips).toHaveLength(3);
    // Столбец 0: низ на bottoms[0]=200, тот же, что якорь — подошва точно на низу первой клетки лесенки (y+1=11).
    expect(strips[0].position[0]).toBe(5);
    expect(strips[0].position[1] + strips[0].size[1]).toBeCloseTo(11, 5);
    // Столбец 1: низ на 40 точек выше (160 vs 200) — подошва на 40/96 клетки выше.
    expect(strips[1].position[0]).toBe(6);
    expect(strips[1].position[1] + strips[1].size[1]).toBeCloseTo(11 - 40 / TILE, 5);
    // Столбец 2: ещё на 40 точек выше.
    expect(strips[2].position[0]).toBe(7);
    expect(strips[2].position[1] + strips[2].size[1]).toBeCloseTo(11 - 80 / TILE, 5);
    for (const s of strips) {
      expect(s.size).toEqual([1, 240 / TILE]);
      expect(s.flip_x).toBeUndefined();
    }
  });

  it("отражённый кусок идёт на запад тем же шагом высоты, и его полоски отражены (flip_x)", () => {
    const objects = buildWallStripObjects({ name: "w", x: 5, y: 10, flip: true }, "wall_log", meta);
    const strips = objects.filter((o) => o.name.startsWith("w_strip_"));
    expect(strips.map((s) => s.position[0])).toEqual([5, 4, 3]); // на запад, не на восток
    for (const s of strips) expect(s.flip_x).toBe(true);
  });

  it("невидимые препятствия — лесенка два вправо на одну вверх, слитые попарно (требование 14)", () => {
    const objects = buildWallStripObjects({ name: "w", x: 5, y: 10, flip: false }, "wall_log", { ...meta, frameCount: 4, bottoms: [200, 160, 120, 80] });
    const walls = objects.filter((o) => o.name.startsWith("w_wall_"));
    // Столбцы 0,1 — та же строка (5,10) и (6,10) → слиты в 2×1; столбцы 2,3 — строка выше (7,9),(8,9) → 2×1.
    expect(walls).toEqual(
      expect.arrayContaining([
        { name: expect.any(String), position: [5, 10], size: [2, 1], layer: 2, obstacle: true },
        { name: expect.any(String), position: [7, 9], size: [2, 1], layer: 2, obstacle: true },
      ]),
    );
  });

  it("целиком прозрачный столбец не рисуется, но всё равно занимает клетку лесенки", () => {
    const objects = buildWallStripObjects({ name: "w", x: 5, y: 10, flip: false }, "wall_log", { bottoms: [200, -1, 120], frameCount: 3, height: 240 });
    const strips = objects.filter((o) => o.name.startsWith("w_strip_"));
    expect(strips.map((s) => s.frame)).toEqual([0, 2]); // столбец 1 пропущен
    const walls = objects.filter((o) => o.name.startsWith("w_wall_"));
    const totalArea = walls.reduce((sum, w) => sum + w.size[0] * w.size[1], 0);
    expect(totalArea).toBe(3); // но лесенка препятствий — все 3 столбца, прозрачный не исключение
  });

  it("часть куска (требование 1 плана): columns берёт только первые столбцы — и полоски, и препятствия", () => {
    const objects = buildWallStripObjects({ name: "w", x: 5, y: 10, flip: false, columns: 2 }, "wall_log", { ...meta, frameCount: 3 });
    expect(objects.filter((o) => o.name.startsWith("w_strip_"))).toHaveLength(2);
    const area = objects.filter((o) => o.name.startsWith("w_wall_")).reduce((sum, w) => sum + w.size[0] * w.size[1], 0);
    expect(area).toBe(2);
  });
});

describe("buildGrassGround — требование 11", () => {
  const [layer] = buildGrassGround();

  it("сетка размером со сцену, набор травы «grass»", () => {
    expect(layer.image).toBe("grass");
    expect(layer.cells).toHaveLength(SCENE_HEIGHT);
    for (const row of layer.cells) expect(row).toHaveLength(SCENE_WIDTH);
  });

  it("плитка берётся по месту клетки в наборе 4×4 (требование 6 фазы, без -1 — трава сплошная)", () => {
    for (let y = 0; y < SCENE_HEIGHT; y++) {
      for (let x = 0; x < SCENE_WIDTH; x++) {
        expect(layer.cells[y][x]).toBe((y % 4) * 4 + (x % 4));
        expect(layer.cells[y][x]).toBeGreaterThanOrEqual(0);
        expect(layer.cells[y][x]).toBeLessThan(16);
      }
    }
  });
});

// Договор с реальным расположением деревни (`rpg-village/layout.mjs`) — тест проходимости по
// прямоугольникам препятствий, план фазы: «мимо живого врага к цели не пройти ни при каком наборе
// убитых врагов». Метаданные кусков — не настоящие файлы (сборка не должна ни читать, ни писать диск
// в тесте): у стен важно только число столбцов из каталога, у ручья и дорожки — размер картинки в
// точках (по нему считается вода).
describe("buildScene(layout) — тест проходимости по прямоугольникам препятствий", () => {
  const SIZE_PX = { path_straight: [978, 505], path_turn: [965, 385], stream: [990, 534], stream_bridge: [992, 534] };
  function fixtureMeta(name) {
    const piece = PIECES[name];
    if (piece.kind === "wallStrip") {
      return { kind: "wallStrip", width: piece.columns * TILE, height: 480, frameCount: piece.columns, bottoms: Array.from({ length: piece.columns }, (_, i) => 470 - i * 30) };
    }
    if (piece.kind === "ground") return { kind: "ground", width: SIZE_PX[name][0], height: SIZE_PX[name][1] };
    return { kind: piece.kind, width: TILE * 3, height: TILE * 3 };
  }
  const pieceMeta = new Map(Object.keys(PIECES).map((name) => [name, fixtureMeta(name)]));
  const scene = buildScene(pieceMeta);
  const names = scene.objects.filter((o) => o.enemy_unit).map((o) => o.name);
  const [goblin1, goblin2, orc] = names;

  it("не больше 200 препятствий (нефункциональное требование фазы)", () => {
    expect(scene.objects.filter((o) => o.obstacle).length).toBeLessThanOrEqual(200);
  });

  it("герой, отметка и враги из layout.mjs: два гоблина и орк, в порядке прохода", () => {
    expect(scene.objects.filter((o) => o.hero)).toHaveLength(1);
    expect(scene.objects.filter((o) => o.marker)).toHaveLength(1);
    expect(ENEMIES.map((e) => e.enemy)).toEqual(["goblin", "goblin", "orc"]);
    expect(names).toEqual(["goblin_1", "goblin_2", "orc_1"]);
  });

  function reachableFrom(deadNames, [x, y], extraBlocking = []) {
    const blocking = [...scene.objects.filter((o) => o.obstacle && !deadNames.has(o.name)), ...extraBlocking];
    const visited = reachableCells(SCENE_WIDTH, SCENE_HEIGHT, blocking, Math.floor(HERO_START.x), Math.floor(HERO_START.y));
    return isReachable(visited, x, y);
  }

  // Клетки по ту сторону каждого прохода: сразу за первым гоблином (внешняя стена), в коридоре за
  // вторым (перегородка), во дворе кузницы за орком.
  const BEYOND_GOBLIN_1 = [21, 7];
  const BEYOND_GOBLIN_2 = [29, 10];
  const SMITHY_YARD = [29, 4];

  // План фазы, критерий готовности: «мимо живого врага к цели не пройти ни при каком наборе убитых
  // врагов» — перебор всех 2³ наборов. Проходы идут один за другим, обхода нет: клетка за проходом
  // достижима тогда и только тогда, когда мёртвы враги во всех проходах до неё включительно.
  it("за каждым проходом достижимо ровно тогда, когда убит его враг и все враги перед ним — все 8 наборов", () => {
    for (let mask = 0; mask < 1 << names.length; mask++) {
      const dead = new Set(names.filter((_, i) => mask & (1 << i)));
      expect(reachableFrom(dead, BEYOND_GOBLIN_1), `за гоблином 1, убиты: ${[...dead]}`).toBe(dead.has(goblin1));
      expect(reachableFrom(dead, BEYOND_GOBLIN_2), `за гоблином 2, убиты: ${[...dead]}`).toBe(dead.has(goblin1) && dead.has(goblin2));
      expect(reachableFrom(dead, SMITHY_YARD), `двор кузницы, убиты: ${[...dead]}`).toBe(dead.size === names.length);
    }
  });

  it("пока орк жив, двор кузницы недостижим при любом наборе убитых гоблинов", () => {
    for (const goblins of [[], [goblin1], [goblin2], [goblin1, goblin2]]) {
      expect(reachableFrom(new Set(goblins), SMITHY_YARD)).toBe(false);
    }
    expect(reachableFrom(new Set([goblin1, goblin2, orc]), SMITHY_YARD)).toBe(true);
  });

  it("герой выходит из нижнего левого угла только по мосту: заткнуть настил — за мостом не пройти", () => {
    const bridge = scene.objects.find((o) => o.image === "stream_bridge");
    const beyondBridge = [12, 12];
    const allAlive = new Set();
    expect(reachableFrom(allAlive, beyondBridge)).toBe(true);
    const deckPlug = { position: [bridge.position[0] + 3, bridge.position[1] + 1], size: [6, 4], obstacle: true };
    expect(reachableFrom(allAlive, beyondBridge, [deckPlug])).toBe(false);
  });

  it("настил моста проходим: на нём нет ни воды, ни другого препятствия", () => {
    const blocking = scene.objects.filter((o) => o.obstacle);
    const deckCell = { x: 7, y: 16 }; // по дорожке, посередине настила (картинка моста: колонки 3–7 из 0–10)
    expect(blocking.some((r) => deckCell.x + 1 > r.position[0] && deckCell.x < r.position[0] + r.size[0] && deckCell.y + 1 > r.position[1] && deckCell.y < r.position[1] + r.size[1])).toBe(false);
  });

  it("слои: дорожки и ручей ниже всех объектов, отметка щелчка — выше них, ниже героя и построек", () => {
    const layerOf = (image) => scene.objects.find((o) => o.image === image).layer;
    for (const image of ["path_straight", "stream", "stream_bridge"]) expect(layerOf(image)).toBe(GROUND_LAYER);
    const marker = scene.objects.find((o) => o.marker);
    expect(marker.layer).toBeGreaterThan(GROUND_LAYER);
    expect(marker.layer).toBeLessThan(layerOf("hero"));
    expect(layerOf("hero")).toBe(OBSTACLE_LAYER);
  });

  it("у дорожки и ручья нет obstacle — они не мешают ходить", () => {
    for (const image of ["path_straight", "stream", "stream_bridge"]) {
      for (const o of scene.objects.filter((x) => x.image === image)) expect(o.obstacle).toBeUndefined();
    }
  });

  it("ручей идёт от края сцены до края: касается нижнего края и заходит за левый", () => {
    const streams = scene.objects.filter((o) => o.image === "stream" || o.image === "stream_bridge");
    expect(streams.length).toBeGreaterThanOrEqual(3);
    expect(Math.max(...streams.map((o) => o.position[1] + o.size[1]))).toBeCloseTo(SCENE_HEIGHT, 1);
    expect(Math.min(...streams.map((o) => o.position[0]))).toBeLessThan(0);
  });

  it("проходы свободны, когда враг убит: на клетки врага не наезжает ни одно другое препятствие", () => {
    const dead = new Set(names);
    for (const enemy of scene.objects.filter((o) => o.enemy_unit)) {
      const blockers = scene.objects.filter(
        (o) => o.obstacle && !dead.has(o.name) && o.position[0] < enemy.position[0] + enemy.size[0] && o.position[0] + o.size[0] > enemy.position[0] && o.position[1] < enemy.position[1] + enemy.size[1] && o.position[1] + o.size[1] > enemy.position[1],
      );
      expect(blockers.map((o) => o.name), enemy.name).toEqual([]);
    }
  });

  // Движок отказывается грузить сцену, если прямоугольник объекта не пересекается со сценой хотя бы
  // частично (engine/tests/rpg_game.rs, «стоит за пределами сцены»).
  it("каждый объект пересекается со сценой (иначе движок откажется грузить scene.json)", () => {
    for (const object of scene.objects) {
      const [x, y] = object.position;
      const [width, height] = object.size;
      expect(x + width > 0 && x < SCENE_WIDTH && y + height > 0 && y < SCENE_HEIGHT, `${object.name}: [${x}, ${y}]..[${x + width}, ${y + height}] вне сцены [0, 0]..[${SCENE_WIDTH}, ${SCENE_HEIGHT}]`).toBe(true);
    }
  });
});

describe("validatePiece — требование 6: неизвестный вид или слой останавливает сборку с именем куска", () => {
  it("неизвестный вид — сообщение с именем куска", () => {
    expect(() => validatePiece("wall_gold", { kind: "hologram", layer: 2 })).toThrow(/wall_gold.*вид/);
  });

  it("не задан слой — сообщение с именем куска", () => {
    expect(() => validatePiece("bush_x", { kind: "foliage" })).toThrow(/bush_x.*слой/);
  });

  it("все куски каталога проходят проверку", () => {
    for (const [name, piece] of Object.entries(PIECES)) expect(() => validatePiece(name, piece)).not.toThrow();
  });
});

describe("каталог — требования 5 и 9", () => {
  it("у стен лесенка препятствий лежит в каталоге, по одной клетке на столбец", () => {
    for (const piece of Object.values(PIECES).filter((p) => p.kind === "wallStrip")) expect(piece.obstacleCells).toHaveLength(piece.columns);
  });

  it("у ручья вода есть, у моста — с разрывом под настилом", () => {
    expect(PIECES.stream.waterCells.length).toBeGreaterThan(PIECES.stream_bridge.waterCells.length);
    const bridgeColumns = new Set(PIECES.stream_bridge.waterCells.map((c) => c.x));
    for (const x of [3, 4, 5, 6, 7]) expect(bridgeColumns.has(x)).toBe(false);
  });

  it("у куста, валуна, столба, ели и берёзы размер в клетках задан целью в каталоге", () => {
    for (const name of ["bush", "boulder", "post", "spruce", "birch"]) expect(PIECES[name].targetSize.cells).toBeGreaterThan(0);
  });
});

describe("mergeVillageCredits — повторный запуск не копит блок деревни", () => {
  const base = "Авторы графики ролевой игры (LPC)\n\nГерой — тело\n  Источник: ...\n";

  it("дописывает блок деревни один раз к прежним записям", () => {
    const merged = mergeVillageCredits(base);
    expect(merged.startsWith(base.trimEnd())).toBe(true);
    expect(merged.match(/Деревня — трава/g)).toHaveLength(1);
  });

  it("повторное слияние заменяет прежний блок деревни, а не добавляет второй", () => {
    const merged = mergeVillageCredits(mergeVillageCredits(base));
    expect(merged.match(/Деревня — трава/g)).toHaveLength(1);
    expect(merged.startsWith(base.trimEnd())).toBe(true);
  });
});
