/**
 * Файл `motions.json` — «Движения» → «Движение с поправками»: таблица «имя → описание движения». Здесь он читается для экрана
 * «Анимация» и правится текстом: каждая правка отдаёт новый текст файла, остальные ключи описаний (`reference`) остаются как были.
 */

export type BoneTurn = readonly [number, number, number];
/** Отметка меняющейся поправки: доля движения от 0 до 1 и поворот кости в ней. */
export type TurnMark = { at: number; turn: BoneTurn };
/** Поправка кости: одна тройка на всё движение или отметки по возрастанию доли. */
export type BoneCorrection = BoneTurn | readonly TurnMark[];

export type MotionSettingKey = "speed" | "bounce" | "lean" | "arms_out" | "loop" | "upper";

export type MotionSettings = {
  model: string;
  clip: string;
  speed: number;
  /** `null` — ключа нет: движение повторяется или нет по тому, где его назвали. */
  loop: boolean | null;
  upper: string | null;
  bounce: number;
  lean: number;
  armsOut: number;
};

export const EMPTY_MOTIONS_TEXT = "{}\n";
/** Доли ближе этого — одна и та же отметка: шаг кадра никогда не бывает меньше. */
export const MARK_TOLERANCE = 0.002;
const MARK_DECIMALS = 10_000;
const INLINE_LIMIT = 100;
/** Таблица и описания движений записываются по строкам; в строку идут только вложенные тройки, отметки и короткие таблицы костей. */
const INLINE_FROM_INDENT = 4;

type MotionTable = Record<string, Record<string, unknown>>;

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isTurn(value: unknown): value is BoneTurn {
  return Array.isArray(value) && value.length === 3 && value.every(isFiniteNumber);
}

function isMark(value: unknown): value is TurnMark {
  return isRecord(value) && isFiniteNumber(value.at) && isTurn(value.turn);
}

function readTable(text: string | null): MotionTable {
  if (text === null) return {};
  try {
    const parsed: unknown = JSON.parse(text);
    if (!isRecord(parsed)) return {};
    return Object.fromEntries(Object.entries(parsed).filter((entry): entry is [string, Record<string, unknown>] => isRecord(entry[1])));
  } catch {
    return {};
  }
}

/** Имена движений файла в порядке записи; файл не читается — пусто. */
export function readMotionNames(text: string | null): string[] {
  return Object.keys(readTable(text));
}

export function readMotionSettings(text: string | null, name: string): MotionSettings | null {
  const entry = readTable(text)[name];
  if (entry === undefined || typeof entry.model !== "string" || typeof entry.clip !== "string") return null;
  return {
    model: entry.model,
    clip: entry.clip,
    speed: isFiniteNumber(entry.speed) ? entry.speed : 1,
    loop: typeof entry.loop === "boolean" ? entry.loop : null,
    upper: typeof entry.upper === "string" ? entry.upper : null,
    bounce: isFiniteNumber(entry.bounce) ? entry.bounce : 1,
    lean: isFiniteNumber(entry.lean) ? entry.lean : 0,
    armsOut: isFiniteNumber(entry.arms_out) ? entry.arms_out : 0,
  };
}

/** Поправки костей движения по именам костей; то, что не тройка и не отметки, пропускается. */
export function readBoneCorrections(text: string | null, name: string): Record<string, BoneCorrection> {
  const bones = readTable(text)[name]?.bones;
  if (!isRecord(bones)) return {};
  const corrections: Record<string, BoneCorrection> = {};
  for (const [bone, value] of Object.entries(bones)) {
    if (isTurn(value)) corrections[bone] = value;
    else if (Array.isArray(value) && value.length > 0 && value.every(isMark)) corrections[bone] = sortedMarks(value);
  }
  return corrections;
}

export function isMarked(correction: BoneCorrection | undefined): correction is readonly TurnMark[] {
  return correction !== undefined && typeof correction[0] === "object";
}

function sortedMarks(marks: readonly TurnMark[]): TurnMark[] {
  return [...marks].sort((first, second) => first.at - second.at);
}

function roundShare(share: number): number {
  return Math.round(Math.min(1, Math.max(0, share)) * MARK_DECIMALS) / MARK_DECIMALS;
}

/**
 * Поправка кости в доле `phase`: тройка как есть, между отметками кубическая кривая Катмулла — Рома, как её считает движок
 * (`Turns::at`); у повторяющегося движения после последней отметки кривая идёт по кругу к первой, у остальных держится крайняя.
 */
export function effectiveBoneTurn(correction: BoneCorrection | undefined, phase: number, isCyclic: boolean): BoneTurn {
  if (correction === undefined) return [0, 0, 0];
  if (!isMarked(correction)) return correction;
  const marks = correction;
  const last = marks.length - 1;
  const first = marks[0] as TurnMark;
  if (last === 0) return first.turn;
  if (!isCyclic) {
    if (phase <= first.at) return first.turn;
    const final = marks[last] as TurnMark;
    if (phase >= final.at) return final.turn;
  }
  const count = marks.length;
  const pick = (index: number): { at: number; turn: BoneTurn } => {
    if (!isCyclic) return marks[Math.min(Math.max(index, 0), last)] as TurnMark;
    const wraps = Math.floor(index / count);
    const mark = marks[((index % count) + count) % count] as TurnMark;
    return { at: mark.at + wraps, turn: mark.turn };
  };
  let after = 0;
  while (after < count && (marks[after] as TurnMark).at <= phase) after += 1;
  const from = pick(after - 1);
  const to = pick(after);
  const before = pick(after - 2).turn;
  const beyond = pick(after + 1).turn;
  const t = Math.min(1, Math.max(0, (phase - from.at) / Math.max(to.at - from.at, 1e-6)));
  const t2 = t * t;
  const t3 = t2 * t;
  const axis = (k: number): number => {
    const [b, f, o, y] = [before[k] as number, from.turn[k] as number, to.turn[k] as number, beyond[k] as number];
    return 0.5 * (2 * f + (-b + o) * t + (2 * b - 5 * f + 4 * o - y) * t2 + (-b + 3 * f - 3 * o + y) * t3);
  };
  return [axis(0), axis(1), axis(2)];
}

/** Номер отметки в доле `phase` с запасом на шаг кадра; `-1` — в этой доле отметки нет. */
export function markIndexAt(marks: readonly TurnMark[], phase: number): number {
  return marks.findIndex((mark) => Math.abs(mark.at - phase) <= MARK_TOLERANCE);
}

function stringify(value: unknown, indent: string): string {
  if (!Array.isArray(value) && !isRecord(value)) return JSON.stringify(value) ?? "null";
  const items = Array.isArray(value) ? value.map((item) => [null, item] as const) : Object.entries(value).map(([key, item]) => [key, item] as const);
  if (items.length === 0) return Array.isArray(value) ? "[]" : "{}";
  const inner = `${indent}  `;
  const render = ([key, item]: readonly [string | null, unknown]): string => `${key === null ? "" : `${JSON.stringify(key)}: `}${stringify(item, inner)}`;
  const [open, close] = Array.isArray(value) ? ["[", "]"] : ["{", "}"];
  const inline = `${open}${open === "{" ? " " : ""}${items.map(render).join(", ")}${close === "}" ? " " : ""}${close}`;
  const isFlat = items.every(([, item]) => !isRecord(item) && !(Array.isArray(item) && item.some((part) => typeof part === "object")));
  if (isFlat && indent.length >= INLINE_FROM_INDENT && inline.length <= INLINE_LIMIT) return inline;
  return `${open}\n${items.map((item) => `${inner}${render(item)}`).join(",\n")}\n${indent}${close}`;
}

/** Текст файла из таблицы: записи по строкам, тройки и отметки в строку; концы строк как в прежнем тексте. */
export function formatMotionsText(table: MotionTable, previousText: string | null): string {
  const text = `${stringify(table, "")}\n`;
  return previousText?.includes("\r\n") ? text.replace(/\n/g, "\r\n") : text;
}

function editedTable(text: string | null, change: (table: MotionTable) => boolean): string | null {
  const table = readTable(text);
  if (!change(table)) return null;
  return formatMotionsText(table, text);
}

/** Новое движение в конец файла; имя занято или пусто — `null`. Тот же файл с новым движением, а нет файла — новый. */
export function motionsTextWithNewMotion(text: string | null, name: string, model: string, clip: string): string | null {
  return editedTable(text, (table) => {
    if (name === "" || name in table) return false;
    table[name] = { model, clip };
    return true;
  });
}

function setKey(entry: Record<string, unknown>, key: string, value: unknown): boolean {
  if (value === undefined) {
    if (!(key in entry)) return false;
    delete entry[key];
    return true;
  }
  if (JSON.stringify(entry[key]) === JSON.stringify(value)) return false;
  entry[key] = value;
  return true;
}

/** Общая ручка движения; `undefined` убирает ключ. */
export function motionsTextWithSetting(text: string | null, name: string, key: MotionSettingKey, value: number | boolean | string | undefined): string | null {
  return editedTable(text, (table) => {
    const entry = table[name];
    return entry !== undefined && setKey(entry, key, value);
  });
}

function changeBones(text: string | null, name: string, change: (bones: Record<string, unknown>) => boolean): string | null {
  return editedTable(text, (table) => {
    const entry = table[name];
    if (entry === undefined) return false;
    const bones = isRecord(entry.bones) ? { ...entry.bones } : {};
    if (!change(bones)) return false;
    if (Object.keys(bones).length === 0) delete entry.bones;
    else entry.bones = bones;
    return true;
  });
}

function marksOf(bones: Record<string, unknown>, bone: string): TurnMark[] | null {
  const value = bones[bone];
  return Array.isArray(value) && value.length > 0 && value.every(isMark) ? sortedMarks(value) : null;
}

function constantOf(bones: Record<string, unknown>, bone: string): BoneTurn {
  const value = bones[bone];
  return isTurn(value) ? value : [0, 0, 0];
}

function roundedTurn(turn: BoneTurn): [number, number, number] {
  return [Math.round(turn[0] * 100) / 100, Math.round(turn[1] * 100) / 100, Math.round(turn[2] * 100) / 100];
}

/**
 * Ползунок или кольцо поставили кости поворот `turn` в доле `phase`: без отметок это постоянная поправка; при отметках
 * правится отметка в этой доле, а если её нет, ставится новая («Анимация», требование 9).
 */
export function motionsTextWithBoneTurn(text: string | null, name: string, bone: string, phase: number, turn: BoneTurn): string | null {
  return changeBones(text, name, (bones) => {
    const marks = marksOf(bones, bone);
    const next = roundedTurn(turn);
    if (marks === null) return setKey(bones, bone, next);
    const index = markIndexAt(marks, phase);
    if (index >= 0) marks[index] = { at: (marks[index] as TurnMark).at, turn: next };
    else marks.push({ at: roundShare(phase), turn: next });
    return setKey(bones, bone, sortedMarks(marks));
  });
}

/** «Отметка»: меняющаяся поправка в доле `phase` с тем поворотом, какой кость имеет там сейчас; отметка там уже есть — `null`. */
export function motionsTextWithMark(text: string | null, name: string, bone: string, phase: number, isCyclic: boolean): string | null {
  return changeBones(text, name, (bones) => {
    const marks = marksOf(bones, bone);
    const current = roundedTurn(effectiveBoneTurn(marks ?? (bones[bone] === undefined ? undefined : constantOf(bones, bone)), phase, isCyclic));
    if (marks === null) return setKey(bones, bone, [{ at: roundShare(phase), turn: current }]);
    if (markIndexAt(marks, phase) >= 0) return false;
    return setKey(bones, bone, sortedMarks([...marks, { at: roundShare(phase), turn: current }]));
  });
}

/** Отметка номер `index` в порядке по возрастанию доли; последняя отметка уходит в постоянную поправку с её поворотом. */
export function motionsTextWithoutMark(text: string | null, name: string, bone: string, index: number): string | null {
  return changeBones(text, name, (bones) => {
    const marks = marksOf(bones, bone);
    if (marks === null || marks[index] === undefined) return false;
    if (marks.length === 1) return setKey(bones, bone, (marks[0] as TurnMark).turn);
    return setKey(bones, bone, marks.filter((_, at) => at !== index));
  });
}

/** Отметку перетащили на долю `phase`; порядок отметок обновляется. */
export function motionsTextWithMovedMark(text: string | null, name: string, bone: string, index: number, phase: number): string | null {
  return changeBones(text, name, (bones) => {
    const marks = marksOf(bones, bone);
    const mark = marks?.[index];
    if (marks === null || mark === undefined) return false;
    const at = roundShare(phase);
    if (marks.some((other, position) => position !== index && Math.abs(other.at - at) <= MARK_TOLERANCE)) return false;
    return setKey(bones, bone, sortedMarks(marks.map((other, position) => (position === index ? { at, turn: other.turn } : other))));
  });
}

/** Сброс поправок кости; без имени кости — всех сразу. */
export function motionsTextWithoutBones(text: string | null, name: string, bone?: string): string | null {
  return changeBones(text, name, (bones) => {
    if (bone === undefined) {
      const keys = Object.keys(bones);
      for (const key of keys) delete bones[key];
      return keys.length > 0;
    }
    return setKey(bones, bone, undefined);
  });
}
