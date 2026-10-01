// Описание рельефа: проверка и перевод в операции построителя. Ошибка называет операцию по номеру
// и имени и говорит, что не так, — описание пишется руками, и опечатка в ключе не должна молча
// давать другой рельеф.

import { isNumber, isPoint, readLine } from "./curve.mjs";
import { readCovers } from "./covers.mjs";
import { derivedSeed, nameHash } from "./random.mjs";
import { readTint } from "./tint.mjs";

const TOP_KEYS = ["size", "density", "seed", "terrain", "water", "operations", "covers", "tint"];

// Для каждой операции: обязательные и необязательные ключи, кроме `op` и `name`. `sharp` — у операций
// с линией или многоугольником. Операции гор (`strata` и дальше) действуют в горах операции `range`,
// названной их ключом `range`.
const OPERATIONS = {
  noise: { required: ["amplitude", "wavelength"], optional: ["area", "fade", "seed", "sharp"] },
  range: { required: ["foot", "side", "height", "depth"], optional: ["roughness", "wavelength", "warp", "profile", "spurs", "seed", "sharp"] },
  hill: { required: ["at", "radius", "height"], optional: ["irregular", "seed"] },
  pad: { required: ["area"], optional: ["height", "rim", "sharp"] },
  channel: { required: ["line", "width", "bottom"], optional: ["bank", "sharp"] },
  smooth: { required: ["passes"], optional: ["area", "except", "sharp"] },
  strata: {
    required: ["range", "step"],
    optional: ["riser", "tilt", "azimuth", "warp", "wavelength", "plates", "strength", "from", "full", "seed"],
  },
  blocks: { required: ["range", "size", "amount"], optional: ["from", "full", "seed"] },
  cracks: { required: ["range", "size", "width", "depth"], optional: ["from", "full", "seed"] },
  erosion: {
    required: ["range"],
    optional: ["drops", "lifetime", "inertia", "capacity", "erode", "deposit", "evaporate", "gravity", "radius", "seed"],
  },
  talus: { required: ["range"], optional: ["repose", "cliff", "weathering", "iterations"] },
};

const MAX_PASSES = 50;
const DENSITIES = [2, 3, 4];

/** Проверяет описание и возвращает размер, воду, путь файла, операции с кривыми и зёрнами и покрытия. */
export function readPlan(plan) {
  if (plan === null || typeof plan !== "object" || Array.isArray(plan)) throw new Error("описание должно быть объектом JSON");
  const extra = Object.keys(plan).filter((key) => !TOP_KEYS.includes(key));
  if (extra.length > 0) throw new Error(`неизвестный ключ описания «${extra[0]}»`);
  const { size, density = 2, seed, terrain, water, operations, covers, tint } = plan;
  if (!Array.isArray(size) || size.length !== 2 || !size.every((n) => Number.isInteger(n) && n > 0)) {
    throw new Error("size — два целых числа больше нуля: ширина и высота сцены в клетках");
  }
  if (!DENSITIES.includes(density)) throw new Error(`density — точек высот на клетку: ${DENSITIES.join(", ")}`);
  if (!Number.isInteger(seed) || seed < 0 || seed > 0xffffffff) throw new Error("seed — целое число от 0 до 4294967295");
  if (typeof terrain !== "string" || terrain === "") throw new Error("terrain — путь файла рельефа от папки описания");
  if (water !== undefined) {
    const keys = Object.keys(water ?? {});
    if (!isNumber(water?.level) || typeof water?.color !== "string" || !/^#[0-9a-fA-F]{6}$/.test(water.color) || keys.length !== 2) {
      throw new Error('water — { "level": число, "color": "#rrggbb" }');
    }
  }
  if (!Array.isArray(operations)) throw new Error("operations — список операций");
  const named = new Map();
  const read = operations.map((op, index) => {
    const operation = readOperation(op, index, seed, named);
    if (op.name !== undefined) {
      if (named.has(op.name)) throw new Error(`операция ${index + 1} «${op.name}»: это имя уже есть у другой операции`);
      named.set(op.name, operation);
    }
    return operation;
  });
  return {
    size,
    density,
    water,
    terrain,
    operations: read,
    covers: covers === undefined ? undefined : readCovers(covers, { seed, water, named }),
    tint: tint === undefined ? undefined : readTint(tint, { seed, named }),
  };
}

function readOperation(op, index, seed, named) {
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
  const points = (key, closed) => readLine(op[key], key, { closed, sharp: op.sharp === true }, fail);

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
      if (op.profile !== undefined && op.profile !== "foot" && op.profile !== "middle") {
        fail("«profile» — \"foot\" (круче всего у подножия) или \"middle\" (круче всего посередине)");
      }
      return {
        op: "range",
        foot: points("foot", false),
        side: op.side,
        height: number("height"),
        depth: number("depth", { above: 0 }),
        roughness: number("roughness", { min: 0, max: 1, fallback: 0.4 }),
        wavelength: number("wavelength", { above: 0, fallback: 16 }),
        warp: number("warp", { min: 0, fallback: 0 }),
        profile: op.profile ?? "foot",
        spurs: op.spurs === undefined ? undefined : number("spurs", { above: 0 }),
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
    case "smooth":
      if (!Number.isInteger(op.passes) || op.passes < 1 || op.passes > MAX_PASSES) fail(`«passes» — целое число от 1 до ${MAX_PASSES}`);
      if (op.except !== undefined && named.get(op.except)?.op !== "range") fail("«except» — имя операции «range» выше по описанию: горы, которые не сглаживаются");
      return {
        op: "smooth",
        passes: op.passes,
        area: op.area === undefined ? undefined : points("area", true),
        except: op.except === undefined ? undefined : named.get(op.except),
      };
    default:
      return readMountainOperation(op, { number, opSeed, named, fail });
  }
}

/** Крутизна, с которой операция гор начинает действовать (`from`) и действует в полную силу (`full`). */
function readSteep(number, fail, from, full) {
  const steep = { from: number("from", { min: 0, max: 90, fallback: from }), full: number("full", { min: 0, max: 90, fallback: full }) };
  if (steep.from >= steep.full) fail("«from» меньше «full»: градусы крутизны, с которых операция начинает и набирает полную силу");
  return steep;
}

function readMountainOperation(op, { number, opSeed, named, fail }) {
  const range = named.get(op.range);
  if (typeof op.range !== "string" || range?.op !== "range") fail("«range» — имя операции «range» выше по описанию: горы, в которых действует операция");
  switch (op.op) {
    case "strata":
      return {
        op: "strata",
        range,
        step: number("step", { above: 0 }),
        riser: number("riser", { above: 0, max: 1, fallback: 0.35 }),
        tilt: number("tilt", { min: -60, max: 60, fallback: 0 }),
        azimuth: number("azimuth", { fallback: 0 }),
        warp: number("warp", { min: 0, fallback: 0.5 }),
        wavelength: number("wavelength", { above: 0, fallback: 12 }),
        plates: number("plates", { above: 0, fallback: 10 }),
        strength: number("strength", { min: 0, max: 1, fallback: 1 }),
        ...readSteep(number, fail, 25, 45),
        seed: opSeed(),
      };
    case "blocks":
      return {
        op: "blocks",
        range,
        size: number("size", { above: 0 }),
        amount: number("amount", { min: 0 }),
        ...readSteep(number, fail, 25, 45),
        seed: opSeed(),
      };
    case "cracks":
      return {
        op: "cracks",
        range,
        size: number("size", { above: 0 }),
        width: number("width", { above: 0 }),
        depth: number("depth", { min: 0 }),
        ...readSteep(number, fail, 30, 50),
        seed: opSeed(),
      };
    case "erosion":
      return {
        op: "erosion",
        range,
        drops: number("drops", { above: 0, fallback: 4 }),
        lifetime: number("lifetime", { above: 0, fallback: 80 }),
        inertia: number("inertia", { min: 0, max: 1, fallback: 0.3 }),
        capacity: number("capacity", { above: 0, fallback: 2 }),
        erode: number("erode", { min: 0, max: 1, fallback: 0.1 }),
        deposit: number("deposit", { min: 0, max: 1, fallback: 0.2 }),
        evaporate: number("evaporate", { min: 0, max: 1, fallback: 0.02 }),
        gravity: number("gravity", { min: 0, fallback: 4 }),
        radius: number("radius", { above: 0, fallback: 0.75 }),
        seed: opSeed(),
      };
    default:
      return {
        op: "talus",
        range,
        repose: number("repose", { above: 0, max: 89, fallback: 35 }),
        cliff: number("cliff", { above: 0, max: 89, fallback: 55 }),
        weathering: number("weathering", { min: 0, fallback: 0.02 }),
        iterations: number("iterations", { min: 1, max: 1000, fallback: 60 }),
      };
  }
}
