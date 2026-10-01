import { shiftedMountainCopy } from "./mountainGeometry";
import type { MountainEntry } from "./terrainFile";

/** Новый список гор и, если действие сдвигает выбор, номер выбранной горы (`null` — выбор снят). */
export type MountainChange = { mountains: MountainEntry[]; selected?: number | null };

/** Новая гора дописывается в конец и выбирается. */
export function mountainsWithPlaced(mountains: readonly MountainEntry[], entry: MountainEntry): MountainChange {
  return { mountains: [...mountains, entry], selected: mountains.length };
}

/** Гора по номеру заменена целиком; такой горы нет — `null`: действия нет. */
export function mountainsWithReplaced(mountains: readonly MountainEntry[], index: number, entry: MountainEntry): MountainChange | null {
  if (mountains[index] === undefined) return null;
  return { mountains: mountains.map((mountain, position) => (position === index ? entry : mountain)) };
}

/** Свойство горы: `undefined` убирает ключ (пустой `rotation`); значение то же, что было, — `null`: действия нет. */
export function mountainsWithValue(mountains: readonly MountainEntry[], index: number, key: string, value: unknown): MountainChange | null {
  const mountain = mountains[index];
  if (mountain === undefined || JSON.stringify(mountain[key]) === JSON.stringify(value)) return null;
  const changed = { ...mountain };
  if (value === undefined) delete changed[key];
  else changed[key] = value;
  return mountainsWithReplaced(mountains, index, changed);
}

/** Копия дописывается в конец со сдвигом на клетку по `x` и выбирается. */
export function mountainsWithCopy(mountains: readonly MountainEntry[], index: number): MountainChange | null {
  const mountain = mountains[index];
  return mountain === undefined ? null : { mountains: [...mountains, shiftedMountainCopy(mountain)], selected: mountains.length };
}

/** Гора убрана, выбор снят. */
export function mountainsWithout(mountains: readonly MountainEntry[], index: number): MountainChange | null {
  return mountains[index] === undefined ? null : { mountains: mountains.filter((_, position) => position !== index), selected: null };
}
