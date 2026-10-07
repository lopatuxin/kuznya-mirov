import type { WorldObjectSummary } from "./battleTypes";
import { applyParticles, type ParticleTable } from "./particlesFile";
import { applyWind, type SceneWind, type WindEditor } from "./sceneWind";

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
  | { kind: "move"; id: number; generation: number; previous: readonly [number, number] }
  | { kind: "transform"; id: number; generation: number; changes: LiveTransformChange[] }
  | { kind: "wind"; previous: SceneWind; next: SceneWind }
  | { kind: "particles"; previous: ParticleTable };

/** Свойство, которое поменял жест ручки; `hadKey` `false` — свойства не было, отмена его убирает. */
type LiveTransformChange = { key: string; hadKey: boolean; previous: unknown };

export type LiveEditHistory = readonly LiveEditEntry[];

/** Вызовы движка, которыми отменяется правка свойств объекта. */
export type LivePropertyEditor = {
  set_property(id: number, name: string, value: unknown): unknown;
  remove_property(id: number, name: string): unknown;
};

/**
 * Отмена жеста ручки на паузе — «Редактор», требование 17: каждому свойству возвращается прежнее
 * значение, а свойство, которого до жеста не было, убирается.
 */
export function undoLiveTransform(entry: { id: number; changes: LiveTransformChange[] }, editor: LivePropertyEditor): void {
  for (const change of entry.changes) {
    if (change.hadKey) editor.set_property(entry.id, change.key, change.previous);
    else editor.remove_property(entry.id, change.key);
  }
}

/** Отмена правки ветра на ходу — «Редактор», требование 32: прежний ветер ставится тем же вызовом движка, что и правка. */
export function undoLiveWind(entry: { previous: SceneWind }, editor: WindEditor): void {
  applyWind(editor, entry.previous);
}

/** Отмена правки видов частиц на ходу — «Редактор», требование 35: прежние виды ставятся тем же вызовом движка, что и правка. */
export function undoLiveParticles(entry: { previous: ParticleTable }, editor: WindEditor): void {
  applyParticles(editor, entry.previous);
}

/**
 * Цела ли ещё запись отмены — «Редактор», требование 21, крайний случай: объект, к которому она
 * относится, уже не тот (правило его удалило, а номер занял новый) — отмена должна снять запись без
 * действия, а не сработать над чужим объектом. `delete` восстанавливает объект заново, поэтому её
 * цель всегда «жива» в этом смысле — сама запись и есть свидетельство, что объекта сейчас нет; ветер
 * и виды частиц к объекту не привязаны вовсе.
 */
export function isLiveEditTargetAlive(entry: LiveEditEntry, worldObjects: readonly WorldObjectSummary[]): boolean {
  if (entry.kind === "delete" || entry.kind === "wind" || entry.kind === "particles") return true;
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
