import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseLocationMap } from "./rpgLocationMap.mjs";
import { buildGroundLayers, buildObjects, buildScene } from "./buildRpgScene.mjs";

const scriptDir = fileURLToPath(new URL(".", import.meta.url));
const REAL_LOCATION_TEXT = readFileSync(resolve(scriptDir, "rpg-lpc/location.txt"), "utf8");

// Маленькая карта для точечных проверок: рамка стен, вход, один гоблин на проёме шириной 2,
// поляна с прудом слева, полянка с камнем справа. Блок стен 2×2 сверху проверяет, что стены
// сливаются в прямоугольник, а не в отдельные полосы.
const FIXTURE = ["#######", "##....#", "##H..~#", "#....##", "#.11.##", "#######"].join("\n") + "\n";

describe("buildGroundLayers", () => {
  const map = parseLocationMap(FIXTURE);
  const [grass, feature, decoration] = buildGroundLayers(map);

  it("три слоя, каждый — сетка нужного размера", () => {
    expect(grass.cells.length).toBe(map.height);
    expect(grass.cells[0].length).toBe(map.width);
    expect(feature.cells.length).toBe(map.height);
    expect(decoration.cells.length).toBe(map.height);
  });

  it("слой травы заполнен на каждой клетке (номер кадра не отрицателен)", () => {
    for (const row of grass.cells) for (const n of row) expect(n).toBeGreaterThanOrEqual(0);
  });

  it("клетка тропинки и клетка воды получают кадр по слою признака, стена — пусто", () => {
    expect(feature.cells[2][2]).toBeGreaterThanOrEqual(0); // 'H' — по-тропиночному
    expect(feature.cells[2][5]).toBeGreaterThanOrEqual(0); // '~'
    expect(feature.cells[0][0]).toBe(-1); // '#'
  });

  it("тропинка и вода — разные наборы кадров (не пересекаются по номеру)", () => {
    const pathFrame = feature.cells[2][2];
    const waterFrame = feature.cells[2][5];
    expect(pathFrame).toBeLessThan(16);
    expect(waterFrame).toBeGreaterThanOrEqual(16);
  });

  it("все слои — набор плиток terrain", () => {
    for (const layer of [grass, feature, decoration]) expect(layer.image).toBe("terrain");
  });
});

describe("buildObjects", () => {
  const map = parseLocationMap(FIXTURE);
  const objects = buildObjects(map);

  it("герой — footprint 0,75 × 0,5 по центру клетки входа, кадр стойки лицом вниз (требование 17, 6)", () => {
    const hero = objects.find((o) => o.hero);
    expect(hero.size).toEqual([0.75, 0.5]);
    expect(hero.position).toEqual([2 + (1 - 0.75) / 2, 2 + (1 - 0.5) / 2]);
    expect(hero).toMatchObject({
      camera_follows: true,
      walk_speed: 4,
      image: "hero",
      frame: 18,
      health: 100,
      max_health: 100,
      damage: 10,
    });
    expect(hero.keys).toEqual({ MouseLeft: { press: [["walk_to", "cursor"]] } });
  });

  it("враг (гоблин) — enemy_unit, obstacle, тело 0,9 × 1,5 по центру проёма шириной 2, кадр стойки лицом вниз", () => {
    const goblin = objects.find((o) => o.enemy_unit);
    expect(goblin.enemy).toBe("goblin");
    expect(goblin.obstacle).toBe(true);
    expect(goblin.size).toEqual([0.9, 1.5]);
    expect(goblin.frame).toBe(18);
    // Проём — клетки (2,4) и (3,4): прямоугольник x∈[2,4), y∈[4,5) — тело центрировано в нём.
    expect(goblin.position[0]).toBeCloseTo(2 + (2 - 0.9) / 2, 5);
    expect(goblin.position[1]).toBeCloseTo(4 + (1 - 1.5) / 2, 5);
    expect(goblin.keys).toEqual({ MouseLeft: { press: [["target", false]] } });
    expect(goblin.on_click).toEqual([["target", true]]);
  });

  it("вода — невидимое препятствие прямоугольником по клетке воды, без image/color", () => {
    const water = objects.find((o) => o.name === "water_0");
    expect(water.obstacle).toBe(true);
    expect(water.image).toBeUndefined();
    expect(water.color).toBeUndefined();
    expect(water.position).toEqual([5, 2]);
    expect(water.size).toEqual([1, 1]);
  });

  it("стены сливаются в прямоугольники, включая блок толще одной клетки", () => {
    const walls = objects.filter((o) => o.image && o.image.startsWith("wall_"));
    // Блок верхних стен 7×2 (строки 0–1) — один или несколько прямоугольников площадью больше 1×1.
    expect(walls.some((w) => w.size[0] * w.size[1] > 1)).toBe(true);
    const totalWallCells = FIXTURE.replace(/\n/g, "")
      .split("")
      .filter((c) => c === "#").length;
    const totalArea = walls.reduce((sum, w) => sum + w.size[0] * w.size[1], 0);
    expect(totalArea).toBe(totalWallCells); // ни одна стена не посчитана дважды и не потеряна
    expect(walls.length).toBeLessThan(totalWallCells);
  });

  it("отметка щелчка — как в фазе 01: прозрачна, ждёт MouseLeft", () => {
    const marker = objects.find((o) => o.marker);
    expect(marker.opacity).toBe(0);
    expect(marker.image).toBe("marker");
  });
});

describe("buildScene — реальная карта location.txt", () => {
  const scene = buildScene(REAL_LOCATION_TEXT);
  const map = parseLocationMap(REAL_LOCATION_TEXT);

  it("не больше 100 препятствий (требование 13)", () => {
    const obstacles = scene.objects.filter((o) => o.obstacle);
    expect(obstacles.length).toBeLessThanOrEqual(100);
  });

  it("размер сцены равен размеру карты (требование 9/12: из location.txt, не из чисел в скрипте)", () => {
    expect(map.width).toBe(32);
    expect(map.height).toBe(24);
  });

  it("ровно один герой, одна отметка, три гоблина и один орк", () => {
    expect(scene.objects.filter((o) => o.hero)).toHaveLength(1);
    expect(scene.objects.filter((o) => o.marker)).toHaveLength(1);
    expect(scene.objects.filter((o) => o.enemy === "goblin")).toHaveLength(3);
    expect(scene.objects.filter((o) => o.enemy === "orc")).toHaveLength(1);
  });

  it("каждый враг целиком внутри своего проёма — тело не заходит в соседнюю стену", () => {
    // Стена — единственная сплошная преграда рядом с проёмом; враг должен помещаться строго между
    // ближайшими стенами по обеим осям (тело шириной 0,9 в проёме шириной 2 и высотой хотя бы 1).
    const walls = scene.objects.filter((o) => o.image && o.image.startsWith("wall_"));
    function overlapsWall(enemy) {
      return walls.some(
        (w) =>
          enemy.position[0] + enemy.size[0] > w.position[0] &&
          enemy.position[0] < w.position[0] + w.size[0] &&
          enemy.position[1] + enemy.size[1] > w.position[1] &&
          enemy.position[1] < w.position[1] + w.size[1],
      );
    }
    for (const enemy of scene.objects.filter((o) => o.enemy_unit)) {
      expect(overlapsWall(enemy)).toBe(false);
    }
  });

  it("каждый враг стоит на кадре стойки лицом вниз, как герой", () => {
    for (const enemy of scene.objects.filter((o) => o.enemy_unit)) expect(enemy.frame).toBe(18);
  });

  it("ширина тела одинакова у всех врагов (0,9), высота — от ступней до макушки этого персонажа (требование 21, ревью пункт 9)", () => {
    // Измерено на кадре стойки вниз основного листа: у гоблина макушка на 14-й точке из 64, у
    // орка — на 17-й, у обоих ступни на 61-й (см. `ENEMY_BODY_HEIGHT` в buildRpgScene.mjs) — так
    // полоска здоровья над рамкой встаёт на одинаковом расстоянии над головой у обоих персонажей.
    const goblin = scene.objects.find((o) => o.enemy === "goblin");
    const orc = scene.objects.find((o) => o.enemy === "orc");
    expect(goblin.size[0]).toBe(0.9);
    expect(orc.size[0]).toBe(0.9);
    expect(goblin.size[1]).toBeCloseTo((61 - 14 + 1) / 32, 5);
    expect(orc.size[1]).toBeCloseTo((61 - 17 + 1) / 32, 5);
    expect(orc.size[1]).not.toBe(goblin.size[1]);
  });

  it("без врагов из входа достижима каждая проходимая клетка", () => {
    const enemyNames = new Set(scene.objects.filter((o) => o.enemy_unit).map((o) => o.name));
    const blocking = scene.objects.filter((o) => o.obstacle && !enemyNames.has(o.name));

    function isBlocked(x, y) {
      return blocking.some(
        (o) => x + 1 > o.position[0] && x < o.position[0] + o.size[0] && y + 1 > o.position[1] && y < o.position[1] + o.size[1],
      );
    }
    const hero = scene.objects.find((o) => o.hero);
    const startX = Math.floor(hero.position[0]);
    const startY = Math.floor(hero.position[1]);
    const visited = Array.from({ length: map.height }, () => new Array(map.width).fill(false));
    const queue = [[startX, startY]];
    visited[startY][startX] = true;
    let reached = 0;
    while (queue.length) {
      const [x, y] = queue.pop();
      reached++;
      for (const [dx, dy] of [
        [1, 0],
        [-1, 0],
        [0, 1],
        [0, -1],
      ]) {
        const nx = x + dx;
        const ny = y + dy;
        if (nx < 0 || ny < 0 || nx >= map.width || ny >= map.height || visited[ny][nx]) continue;
        if (isBlocked(nx, ny)) continue;
        visited[ny][nx] = true;
        queue.push([nx, ny]);
      }
    }
    let totalOpen = 0;
    for (let y = 0; y < map.height; y++) for (let x = 0; x < map.width; x++) if (!isBlocked(x, y)) totalOpen++;
    expect(reached).toBe(totalOpen);
  });

  // Дальняя поляна — требование 10: комнаты c 8–9, r 2–4, то есть клетки x∈[25,29], y∈[8,15].
  const FAR_GLADE_RECT = { x0: 25, x1: 29, y0: 8, y1: 15 };

  function reachableFromHero(extraBlockedNames) {
    const blocking = scene.objects.filter((o) => o.obstacle && !extraBlockedNames.has(o.name));
    function isBlocked(x, y) {
      return blocking.some(
        (o) => x + 1 > o.position[0] && x < o.position[0] + o.size[0] && y + 1 > o.position[1] && y < o.position[1] + o.size[1],
      );
    }
    const hero = scene.objects.find((o) => o.hero);
    const visited = Array.from({ length: map.height }, () => new Array(map.width).fill(false));
    const startX = Math.floor(hero.position[0]);
    const startY = Math.floor(hero.position[1]);
    const queue = [[startX, startY]];
    visited[startY][startX] = true;
    while (queue.length) {
      const [x, y] = queue.pop();
      for (const [dx, dy] of [
        [1, 0],
        [-1, 0],
        [0, 1],
        [0, -1],
      ]) {
        const nx = x + dx;
        const ny = y + dy;
        if (nx < 0 || ny < 0 || nx >= map.width || ny >= map.height || visited[ny][nx]) continue;
        if (isBlocked(nx, ny)) continue;
        visited[ny][nx] = true;
        queue.push([nx, ny]);
      }
    }
    return visited;
  }

  function farGladeReachable(visited) {
    for (let y = FAR_GLADE_RECT.y0; y <= FAR_GLADE_RECT.y1; y++) {
      for (let x = FAR_GLADE_RECT.x0; x <= FAR_GLADE_RECT.x1; x++) {
        if (visited[y][x]) return true;
      }
    }
    return false;
  }

  it("с врагами дальняя поляна недостижима ни одним путём", () => {
    expect(farGladeReachable(reachableFromHero(new Set()))).toBe(false);
  });

  it("дальняя поляна достижима тогда и только тогда, когда убраны оба врага верхнего пути (goblin_1 и orc_1) или оба врага нижнего (goblin_2 и goblin_3) — все 16 сочетаний", () => {
    // Ревью: до правки в поляну вёл третий (а затем и четвёртый) проём в обход орка/goblin_3,
    // потому что глухая поляна получала в дереве маршрутов степень 2 (сквозной проход насквозь
    // через её открытое нутро), а не степень 1 (тупиковая ветка) — герой обходил охрану.
    const enemies = ["goblin_1", "orc_1", "goblin_2", "goblin_3"];
    for (let mask = 0; mask < 16; mask++) {
      const removed = new Set(enemies.filter((_, i) => mask & (1 << i)));
      const isRemoved = (name) => removed.has(name);
      const expected = (isRemoved("goblin_1") && isRemoved("orc_1")) || (isRemoved("goblin_2") && isRemoved("goblin_3"));
      const actual = farGladeReachable(reachableFromHero(removed));
      expect(actual).toBe(expected);
    }
  });

  it("в дальнюю поляну ведут ровно два проёма (требование 10/11)", () => {
    // Проём — структурная открытость (стена убрана), не зависит от того, стоит ли там сейчас враг:
    // враг — временный объект боя, а не часть формы лабиринта. Считаем не клетки, а проёмы:
    // соседние открытые клетки периметра, ведущие наружу, — один и тот же проём шириной 2.
    const blocking = scene.objects.filter((o) => o.image && o.image.startsWith("wall_"));
    function isBlocked(x, y) {
      return blocking.some(
        (o) => x + 1 > o.position[0] && x < o.position[0] + o.size[0] && y + 1 > o.position[1] && y < o.position[1] + o.size[1],
      );
    }
    const outwardOpenCells = new Set();
    for (let y = FAR_GLADE_RECT.y0; y <= FAR_GLADE_RECT.y1; y++) {
      for (let x = FAR_GLADE_RECT.x0; x <= FAR_GLADE_RECT.x1; x++) {
        if (isBlocked(x, y)) continue;
        for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
          const nx = x + dx;
          const ny = y + dy;
          const outsideRect = nx < FAR_GLADE_RECT.x0 || nx > FAR_GLADE_RECT.x1 || ny < FAR_GLADE_RECT.y0 || ny > FAR_GLADE_RECT.y1;
          if (outsideRect && !isBlocked(nx, ny)) outwardOpenCells.add(`${x},${y}`);
        }
      }
    }
    // Смежные по стороне клетки периметра объединяем в один проём (обход связности по набору).
    const cells = [...outwardOpenCells].map((key) => key.split(",").map(Number));
    const visited = new Set();
    let doorways = 0;
    for (const [sx, sy] of cells) {
      const startKey = `${sx},${sy}`;
      if (visited.has(startKey)) continue;
      doorways++;
      const stack = [[sx, sy]];
      visited.add(startKey);
      while (stack.length) {
        const [x, y] = stack.pop();
        for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
          const key = `${x + dx},${y + dy}`;
          if (outwardOpenCells.has(key) && !visited.has(key)) {
            visited.add(key);
            stack.push([x + dx, y + dy]);
          }
        }
      }
    }
    expect(doorways).toBe(2);
  });

  it("внутри полян (дальней, с прудом, с камнями) нет стен — поляна целиком открыта", () => {
    // Прямоугольник поляны — её собственные клетки (требование 10); стены, ограждающие поляну
    // снаружи, лежат ЗА его пределами (например, у дальней поляны — столбец x=24), так что любое
    // пересечение стены с этим прямоугольником — лишняя стена внутри поляны.
    const walls = scene.objects.filter((o) => o.image && o.image.startsWith("wall_"));
    function overlapsRect(wall, rect) {
      return (
        wall.position[0] < rect.x1 + 1 &&
        wall.position[0] + wall.size[0] > rect.x0 &&
        wall.position[1] < rect.y1 + 1 &&
        wall.position[1] + wall.size[1] > rect.y0
      );
    }
    const glades = [
      FAR_GLADE_RECT,
      { x0: 7, x1: 11, y0: 2, y1: 6 }, // поляна с прудом (требование 10)
      { x0: 13, x1: 17, y0: 17, y1: 21 }, // поляна с камнями (требование 10)
    ];
    for (const rect of glades) {
      expect(walls.some((w) => overlapsRect(w, rect))).toBe(false);
    }
  });

  it("не меньше четырёх тупиковых комнат в сетке 10×7 (требование 10)", () => {
    // Комната (c,r) — клетки x∈[1+3c,2+3c], y∈[2+3r,3+3r] (требование 10). Тупик — обычная
    // комната лабиринта (не одна из трёх полян), у которой открыт проём только в одну соседнюю
    // комнату. Полянам (дальняя, с прудом, с камнями) и входу тупиковость не считаем: у них не
    // предполагается ровно один проём.
    const isWall = (x, y) => map.rows[y][x] === "#";
    function roomOpenSides(c, r) {
      let degree = 0;
      if (c + 1 < 10 && !isWall(3 + 3 * c, 2 + 3 * r)) degree++;
      if (c > 0 && !isWall(3 + 3 * (c - 1), 2 + 3 * r)) degree++;
      if (r + 1 < 7 && !isWall(1 + 3 * c, 4 + 3 * r)) degree++;
      if (r > 0 && !isWall(1 + 3 * c, 4 + 3 * (r - 1))) degree++;
      return degree;
    }
    const FAR_GLADE = { c0: 8, c1: 9, r0: 2, r1: 4 };
    const POND_GLADE = { c0: 2, c1: 3, r0: 0, r1: 1 };
    const ROCK_GLADE = { c0: 4, c1: 5, r0: 5, r1: 6 };
    const ENTRANCE = { c: 0, r: 3 };
    function inRect(c, r, rect) {
      return c >= rect.c0 && c <= rect.c1 && r >= rect.r0 && r <= rect.r1;
    }
    let deadEndRooms = 0;
    for (let r = 0; r < 7; r++) {
      for (let c = 0; c < 10; c++) {
        if (inRect(c, r, FAR_GLADE) || inRect(c, r, POND_GLADE) || inRect(c, r, ROCK_GLADE)) continue;
        if (c === ENTRANCE.c && r === ENTRANCE.r) continue;
        if (roomOpenSides(c, r) === 1) deadEndRooms++;
      }
    }
    expect(deadEndRooms).toBeGreaterThanOrEqual(4);
  });
});
