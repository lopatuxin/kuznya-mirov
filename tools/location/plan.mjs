// Описание рельефа: проверка и перевод в операции построителя. Ошибка называет операцию по номеру
// и имени и говорит, что не так, — описание пишется руками, и опечатка в ключе не должна молча
// давать другой рельеф.

import { path } from "./curve.mjs";
import { derivedSeed } from "./random.mjs";

const TOP_KEYS = ["size", "seed", "terrain", "water", "operations"];

// Для каждой операции: обязательные и необязательные ключи, кроме `op` и `name`. `sharp` — у операций
// с линией или многоугольником.
const OPERATIONS = {
  noise: { required: ["amplitude", "wavelength"], optional: ["area", "fade", "seed", "sharp"] },
  range: { required: ["foot", "side", "height", "depth"], optional: ["roughness", "wavelength", "warp", "seed", "sharp"] },
  hill: { required: ["at", "radius", "height"], optional: ["irregular", "seed"] },
  pad: { required: ["area"], optional: ["height", "rim", "sharp"] },
  channel: { required: ["line", "width", "bottom"], optional: ["bank", "sharp"] },
  smooth: { required: ["passes"], optional: ["area", "sharp"] },
};

const MAX_PASSES = 50;

function isNumber(value) {
  return typeof value === "number" && Number.isFinite(value);
}

function isPoint(value) {
  return Array.isArray(value) && value.length === 2 && value.every(isNumber);
}

// Имя операции в число для её зерна: неровности операции зависят от её имени, а не от места в
// описании, и вставка новой операции не меняет соседей.
function nameHash(name) {
  let h = 0x811c9dc5;
  for (let i = 0; i < name.length; i++) h = Math.imul(h ^ name.charCodeAt(i), 0x01000193) >>> 0;
  return h;
}

/** Проверяет описание и возвращает размер, воду, путь файла и операции с кривыми и зёрнами. */
export function readPlan(plan) {
  if (plan === null || typeof plan !== "object" || Array.isArray(plan)) throw new Error("описание должно быть объектом JSON");
  const extra = Object.keys(plan).filter((key) => !TOP_KEYS.includes(key));
  if (extra.length > 0) throw new Error(`неизвестный ключ описания «${extra[0]}»`);
  const { size, seed, terrain, water, operations } = plan;
  if (!Array.isArray(size) || size.length !== 2 || !size.every((n) => Number.isInteger(n) && n > 0)) {
    throw new Error("size — два целых числа больше нуля: ширина и высота сцены в клетках");
  }
  if (!Number.isInteger(seed) || seed < 0 || seed > 0xffffffff) throw new Error("seed — целое число от 0 до 4294967295");
  if (typeof terrain !== "string" || terrain === "") throw new Error("terrain — путь файла рельефа от папки описания");
  if (water !== undefined) {
    const keys = Object.keys(water ?? {});
    if (!isNumber(water?.level) || typeof water?.color !== "string" || !/^#[0-9a-fA-F]{6}$/.test(water.color) || keys.length !== 2) {
      throw new Error('water — { "level": число, "color": "#rrggbb" }');
    }
  }
  if (!Array.isArray(operations)) throw new Error("operations — список операций");
  const names = new Set();
  return {
    size,
    water,
    terrain,
    operations: operations.map((op, index) => {
      const read = readOperation(op, index, seed);
      if (op.name !== undefined) {
        if (names.has(op.name)) throw new Error(`операция ${index + 1} «${op.name}»: это имя уже есть у другой операции`);
        names.add(op.name);
      }
      return read;
    }),
  };
}

function readOperation(op, index, seed) {
  const where = `операция ${index + 1}${typeof op?.name === "string" ? ` «${op.name}»` : ""}`;
  const fail = (message) => {
    throw new Error(`${where}: ${message}`);
  };
  if (op === null || typeof op !== "object" || Array.isArray(op)) fail("операция — объект JSON");
  if (!Object.hasOwn(OPERATIONS, op.op)) fail(`неизвестная операция «${op.op}», есть: ${Object.keys(OPERATIONS).join(", ")}`);
  const spec = OPERATIONS[op.op];
  if (op.name !== undefined && (typeof op.name !== "string" || op.name === "")) fail("«name» — непустой текст");
  const allowed = ["op", "name", ...spec.required, ...spec.optional];
  const extra = Object.keys(op).find((key) => !allowed.includes(key));
  if (extra) fail(`неизвестный ключ «${extra}»`);
  const missing = spec.required.find((key) => op[key] === undefined);
  if (missing) fail(`нет обязательного ключа «${missing}»`);
  if (op.sharp !== undefined && typeof op.sharp !== "boolean") fail("«sharp» — true или false");
  if ((op.op === "noise" || op.op === "smooth") && op.area === undefined) {
    if (op.sharp !== undefined) fail("«sharp» — только вместе с «area»");
    if (op.fade !== undefined) fail("«fade» — только вместе с «area»");
  }

  const number = (key, { min, max, above, fallback } = {}) => {
    const value = op[key] === undefined ? fallback : op[key];
    if (!isNumber(value)) fail(`«${key}» — число`);
    if (min !== undefined && value < min) fail(`«${key}» не меньше ${min}`);
    if (max !== undefined && value > max) fail(`«${key}» не больше ${max}`);
    if (above !== undefined && value <= above) fail(`«${key}» больше ${above}`);
    return value;
  };
  const opSeed = () => {
    if (op.seed !== undefined) {
      if (!Number.isInteger(op.seed) || op.seed < 0 || op.seed > 0xffffffff) fail("«seed» — целое число от 0 до 4294967295");
      return op.seed;
    }
    return derivedSeed(seed, op.name === undefined ? index : nameHash(op.name));
  };
  const points = (key, closed) => {
    const value = op[key];
    const least = closed ? 3 : 2;
    if (!Array.isArray(value) || value.length < least || !value.every(isPoint)) {
      fail(`«${key}» — список не меньше ${least} точек [x, y]`);
    }
    const line = path(value, { closed, sharp: op.sharp === true });
    if (line.segments.length === 0) fail(`«${key}» — все точки совпадают`);
    if (closed) {
      const pts = line.points;
      const area = pts.reduce((sum, p, i) => {
        const q = pts[(i + 1) % pts.length];
        return sum + p[0] * q[1] - q[0] * p[1];
      }, 0);
      if (Math.abs(area) < 1e-9) fail(`«${key}» — многоугольник без площади: точки на одной прямой`);
    }
    return line;
  };

  switch (op.op) {
    case "noise":
      return {
        op: "noise",
        amplitude: number("amplitude"),
        wavelength: number("wavelength", { above: 0 }),
        area: op.area === undefined ? undefined : points("area", true),
        fade: number("fade", { min: 0, fallback: 4 }),
        seed: opSeed(),
      };
    case "range":
      if (op.side !== "left" && op.side !== "right") fail("«side» — \"left\" или \"right\" по ходу линии подножия");
      return {
        op: "range",
        foot: points("foot", false),
        side: op.side,
        height: number("height"),
        depth: number("depth", { above: 0 }),
        roughness: number("roughness", { min: 0, max: 1, fallback: 0.4 }),
        wavelength: number("wavelength", { above: 0, fallback: 16 }),
        warp: number("warp", { min: 0, fallback: 0 }),
        seed: opSeed(),
      };
    case "hill":
      if (!isPoint(op.at)) fail("«at» — точка [x, y]");
      return {
        op: "hill",
        at: op.at,
        radius: number("radius", { above: 0 }),
        height: number("height"),
        irregular: number("irregular", { min: 0, max: 0.9, fallback: 0 }),
        seed: opSeed(),
      };
    case "pad":
      return {
        op: "pad",
        area: points("area", true),
        height: op.height === undefined ? undefined : number("height"),
        rim: number("rim", { min: 0, fallback: 2 }),
      };
    case "channel": {
      const bottom = op.bottom;
      if (!isNumber(bottom) && !(Array.isArray(bottom) && bottom.length === 2 && bottom.every(isNumber))) {
        fail("«bottom» — число или два числа: высота дна в начале и в конце линии");
      }
      return {
        op: "channel",
        line: points("line", false),
        width: number("width", { above: 0 }),
        bottom,
        bank: number("bank", { min: 0, fallback: 1 }),
      };
    }
    default:
      if (!Number.isInteger(op.passes) || op.passes < 1 || op.passes > MAX_PASSES) fail(`«passes» — целое число от 1 до ${MAX_PASSES}`);
      return { op: "smooth", passes: op.passes, area: op.area === undefined ? undefined : points("area", true) };
  }
}
