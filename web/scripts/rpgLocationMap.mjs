// Разбор текстовой карты локации («Фаза-02-враг-на-пути», пункт 9) и слияние соседних клеток
// препятствия в прямоугольники, не только строки/столбцы (пункт 13). Общий модуль:
// buildRpgArt.mjs узнаёт отсюда, картинки стен какого размера собирать, buildRpgScene.mjs — тем же
// способом раскладывает объекты сцены (стены и вода), так что размеры совпадают без повторного
// алгоритма.

export const CELL = {
  WALL: "#",
  TREE: "T",
  WATER: "~",
  ROCK: "*",
  PATH: ".",
  GLADE: ",",
  HERO: "H",
  GOBLIN_1: "1",
  GOBLIN_2: "2",
  GOBLIN_3: "3",
  ORC: "X",
};

export function parseLocationMap(text) {
  const rows = text.replace(/\r\n/g, "\n").split("\n").filter((line) => line.length > 0);
  const height = rows.length;
  const width = rows[0].length;
  for (const row of rows) {
    if (row.length !== width) throw new Error(`location.txt: строки разной длины (${row.length} и ${width})`);
  }
  return { width, height, rows };
}

// Жадное слияние в прямоугольники: для каждой ещё не занятой клетки нужного символа берём
// максимальную ширину вправо, затем максимальную высоту вниз, при которой вся полоса той же
// ширины остаётся тем же символом и не занята — пункт 13: «сливаются в прямоугольники», не только
// в полосы одной строки/столбца (частный случай width=1 или height=1 получается тем же кодом).
export function extractRectangles(map, matchChar) {
  const { width, height, rows } = map;
  const visited = Array.from({ length: height }, () => new Array(width).fill(false));
  const is = (x, y) => x >= 0 && x < width && y >= 0 && y < height && rows[y][x] === matchChar && !visited[y][x];

  const rects = [];
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (!is(x, y)) continue;

      let w = 1;
      while (is(x + w, y)) w++;

      let h = 1;
      rowLoop: while (y + h < height) {
        for (let i = 0; i < w; i++) {
          if (!is(x + i, y + h)) break rowLoop;
        }
        h++;
      }

      for (let j = 0; j < h; j++) {
        for (let i = 0; i < w; i++) visited[y + j][x + i] = true;
      }
      rects.push({ x, y, width: w, height: h });
    }
  }
  return rects;
}
