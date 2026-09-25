import type { WorldObjectSummary } from "./battleTypes";

/**
 * Стек правок на ходу — «Редактор», требование 21: каждая правка над живым миром кладёт сюда, как
 * её откатить; «Стоп» очищает стек целиком (`clearLiveEditHistory`). Отдельно от истории файловой
 * правки (`editSession.ts`) — правки на ходу не пишут файлы и не переживают остановку партии.
 *
 * Каждая запись, кроме `delete`, держит `generation` объекта на момент правки — не только номер:
 * правило может удалить объект и тут же отдать его номер новому (в тетрисе — постоянно), а «жив ли»
 * проверяется парой номер+метка (`isLiveEditTargetAlive`), иначе Ctrl+Z правил бы чужой объект.
 * `delete` эту метку не хранит — объекта с её номером в мире по определению уже нет, это и есть
 * повод её отменять, а не отказ.
 */
export type LiveEditEntry =
  | { kind: "set"; id: number; generation: number; key: string; hadKey: boolean; previous: unknown }
  | { kind: "remove"; id: number; generation: number; key: string; previous: unknown }
  | { kind: "add"; id: number; generation: number }
  | { kind: "delete"; id: number; properties: Record<string, unknown> }
  | { kind: "move"; id: number; generation: number; previous: readonly [number, number] };

export type LiveEditHistory = readonly LiveEditEntry[];

/**
 * Цела ли ещё запись отмены — «Редактор», требование 21, крайний случай: объект, к которому она
 * относится, уже не тот (правило его удалило, а номер занял новый) — отмена должна снять запись без
 * действия, а не сработать над чужим объектом. `delete` восстанавливает объект заново, поэтому её
 * цель всегда «жива» в этом смысле — сама запись и есть свидетельство, что объекта сейчас нет.
 */
export function isLiveEditTargetAlive(entry: LiveEditEntry, worldObjects: readonly WorldObjectSummary[]): boolean {
  if (entry.kind === "delete") return true;
  return worldObjects.some((object) => object.id === entry.id && object.generation === entry.generation);
}

export function createLiveEditHistory(): LiveEditHistory {
  return [];
}

export function pushLiveEdit(history: LiveEditHistory, entry: LiveEditEntry): LiveEditHistory {
  return [...history, entry];
}

/** Последняя правка и остаток стека без неё — `null`, если правок ещё не было (кнопка неактивна). */
export function popLiveEdit(history: LiveEditHistory): { entry: LiveEditEntry; rest: LiveEditHistory } | null {
  if (history.length === 0) return null;
  const entry = history[history.length - 1] as LiveEditEntry;
  return { entry, rest: history.slice(0, -1) };
}
