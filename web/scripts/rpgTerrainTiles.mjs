// Раскладка кадров набора плиток `terrain.png` («Фаза-02-враг-на-пути», пункты 6 и 15). Общий
// модуль: buildRpgArt.mjs рисует кадры по этим индексам, buildRpgScene.mjs — расставляет те же
// номера в `ground.cells` по соседям клетки, так что раскладка не дублируется.
//
// Кадры 0–15 и 16–31 — блочный автотайл по четырём соседям (N=1, E=2, S=4, W=8): бит установлен,
// когда сосед с той же стороны — тоже тропинка (для 0–15) или тоже вода (для 16–31); индекс кадра
// равен самой битовой маске. Кадр 32 — единственный вариант травы (базовый слой везде одинаков;
// ревью: второй вариант оказался вырезкой из фермерского GPL-набора Daniel Eddeland, убран без
// замены — разнообразие даёт не второй фон, а мелочи), 33–34 — мелочи поверх травы.

export const TERRAIN_COLUMNS = 8;

export const NEIGHBOR_BIT = { N: 1, E: 2, S: 4, W: 8 };

export const PATH_TILE_BASE = 0;
export const WATER_TILE_BASE = 16;
export const GRASS_VARIANT_TILES = [32];
export const DECORATION_TILES = [33, 34];
export const TERRAIN_FRAME_COUNT = 35;

export function pathTileIndex(bitmask) {
  return PATH_TILE_BASE + bitmask;
}

export function waterTileIndex(bitmask) {
  return WATER_TILE_BASE + bitmask;
}

// Битовая маска соседей клетки (x, y), которые сами удовлетворяют `isSameType`.
export function neighborBitmask(width, height, x, y, isSameType) {
  let mask = 0;
  if (y > 0 && isSameType(x, y - 1)) mask |= NEIGHBOR_BIT.N;
  if (x < width - 1 && isSameType(x + 1, y)) mask |= NEIGHBOR_BIT.E;
  if (y < height - 1 && isSameType(x, y + 1)) mask |= NEIGHBOR_BIT.S;
  if (x > 0 && isSameType(x - 1, y)) mask |= NEIGHBOR_BIT.W;
  return mask;
}

// Полное перемешивание битов (splitmix-подобный финализатор) — в отличие от простого XOR двух
// нечётных множителей (прежняя реализация), где `% 2` вырождается в чётность `x ^ y` и даёт
// шахматный узор через клетку, здесь младший бит результата зависит от всех бит x и y сразу.
function hashCell(x, y, salt) {
  let h = (x * 0x9e3779b1) ^ (y * 0x85ebca6b) ^ salt;
  h = Math.imul(h ^ (h >>> 15), 0x2545f491);
  h ^= h >>> 13;
  h = Math.imul(h, 0xc2b2ae35);
  h ^= h >>> 16;
  return h >>> 0;
}

// Один вариант травы на весь базовый слой (ревью: второй вариант убран, см. выше) — разнообразие
// даёт не фон, а мелочи (`decorationTile`/`hasDecoration`). Функция остаётся готовой к нескольким
// вариантам (основной на большей части клеток, редкий вразброс, без узора по строкам, столбцам
// или диагоналям — тот же avalanche-хеш, что и раньше), на случай если вариант вернётся.
const GRASS_PRIMARY_WEIGHT = 0.85;

// Детерминированный «случайный» выбор варианта травы по клетке — без состояния, тот же результат
// при каждой сборке.
export function grassVariantTile(x, y) {
  const primaryRoll = hashCell(x, y, 0x1) / 0x100000000;
  if (GRASS_VARIANT_TILES.length === 1 || primaryRoll < GRASS_PRIMARY_WEIGHT) return GRASS_VARIANT_TILES[0];
  const restCount = GRASS_VARIANT_TILES.length - 1;
  const restRoll = hashCell(x, y, 0x9) % restCount;
  return GRASS_VARIANT_TILES[1 + restRoll];
}

// Тот же avalanche-хеш, что и у травы — прежний `(нечёт*x)^(нечёт*y)` тоже вырождался в чётность
// `x^y` под `% 2`.
export function decorationTile(x, y) {
  const hash = hashCell(x, y, 0x51);
  return DECORATION_TILES[hash % DECORATION_TILES.length];
}

// Мелочи разбросаны редко и только по траве полян — не на каждой клетке.
export function hasDecoration(x, y) {
  const hash = hashCell(x, y, 0x7d);
  return hash % 9 === 0;
}
