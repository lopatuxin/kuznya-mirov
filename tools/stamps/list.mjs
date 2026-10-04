// Список вырезов штампов: проверка и значения по умолчанию. Список пишется руками, поэтому ошибка
// называет вырез по номеру и имени и говорит, что не так, а опечатка в ключе не даёт молча другой штамп.

export const DEFAULT_POINTS = 192;
export const DEFAULT_BASE = 0.1;
export const DEFAULT_FADE = 0.55;
export const DEFAULT_ZOOM = 13;

const TOP_KEYS = ["out", "stamps"];
const CUT_REQUIRED = ["name", "place", "lat", "lon", "km"];
const CUT_KEYS = [...CUT_REQUIRED, "points", "base", "fade", "zoom", "invert"];
const MAX_LATITUDE = 85;
const MAX_KM = 100;
const MAX_POINTS = 1024;
const MIN_ZOOM = 10;
const MAX_ZOOM = 15;

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isNumber(value) {
  return typeof value === "number" && Number.isFinite(value);
}

/** Проверяет список и возвращает `{ out, cuts }`; `out` — папка штампов, как записана в списке. */
export function readCutList(list) {
  if (!isObject(list)) throw new Error("список вырезов должен быть объектом JSON");
  const extra = Object.keys(list).find((key) => !TOP_KEYS.includes(key));
  if (extra) throw new Error(`неизвестный ключ списка «${extra}»`);
  if (typeof list.out !== "string" || list.out === "") throw new Error("out — путь папки штампов от файла списка");
  if (!Array.isArray(list.stamps) || list.stamps.length === 0) throw new Error("stamps — непустой список вырезов");
  const names = new Set();
  const cuts = list.stamps.map((cut, index) => {
    const read = readCut(cut, index);
    if (names.has(read.name)) throw new Error(`вырез ${index + 1} «${read.name}»: это имя уже есть у другого выреза`);
    names.add(read.name);
    return read;
  });
  return { out: list.out, cuts };
}

function readCut(cut, index) {
  const where = `вырез ${index + 1}${typeof cut?.name === "string" ? ` «${cut.name}»` : ""}`;
  const fail = (message) => {
    throw new Error(`${where}: ${message}`);
  };
  if (!isObject(cut)) fail("вырез — объект JSON");
  const extra = Object.keys(cut).find((key) => !CUT_KEYS.includes(key));
  if (extra) fail(`неизвестный ключ «${extra}»`);
  const missing = CUT_REQUIRED.find((key) => cut[key] === undefined);
  if (missing) fail(`нет обязательного ключа «${missing}»`);
  if (typeof cut.name !== "string" || !/^[A-Za-z0-9_-]+$/.test(cut.name)) fail("«name» — имя из латинских букв, цифр, «_» и «-»: по нему назван файл");
  if (typeof cut.place !== "string" || cut.place === "") fail("«place» — непустой текст: где вырезан этот штамп");
  if (!isNumber(cut.lat) || Math.abs(cut.lat) > MAX_LATITUDE) fail(`«lat» — широта, число от -${MAX_LATITUDE} до ${MAX_LATITUDE}`);
  if (!isNumber(cut.lon) || Math.abs(cut.lon) > 180) fail("«lon» — долгота, число от -180 до 180");
  if (!Array.isArray(cut.km) || cut.km.length !== 2 || !cut.km.every((km) => isNumber(km) && km > 0 && km <= MAX_KM)) {
    fail(`«km» — два числа больше 0 и не больше ${MAX_KM}: ширина и глубина выреза в километрах`);
  }
  const { points = DEFAULT_POINTS, base = DEFAULT_BASE, fade = DEFAULT_FADE, zoom = DEFAULT_ZOOM, invert = false } = cut;
  if (!Number.isInteger(points) || points < 2 || points > MAX_POINTS) fail(`«points» — целое число от 2 до ${MAX_POINTS}`);
  if (!isNumber(base) || base < 0 || base >= 1) fail("«base» — число от 0 до 1, не включая 1");
  if (!isNumber(fade) || fade <= 0 || fade > 1) fail("«fade» — число больше 0 и не больше 1");
  if (!Number.isInteger(zoom) || zoom < MIN_ZOOM || zoom > MAX_ZOOM) fail(`«zoom» — целое число от ${MIN_ZOOM} до ${MAX_ZOOM}: масштаб плиток`);
  if (typeof invert !== "boolean") fail("«invert» — true или false");
  return { name: cut.name, place: cut.place, lat: cut.lat, lon: cut.lon, km: cut.km, points, base, fade, zoom, invert };
}
