import { shiftedImprintCopy } from "./imprintGeometry";
import type { ImprintEntry } from "./terrainFile";

/** Новый список отпечатков и, если действие сдвигает выбор, номер выбранного отпечатка (`null` — выбор снят). */
export type ImprintChange = { imprints: ImprintEntry[]; selected?: number | null };

/** Новый отпечаток дописывается в конец и выбирается. */
export function imprintsWithPlaced(imprints: readonly ImprintEntry[], entry: ImprintEntry): ImprintChange {
  return { imprints: [...imprints, entry], selected: imprints.length };
}

/** Отпечаток по номеру заменён целиком; такого отпечатка нет — `null`: действия нет. */
export function imprintsWithReplaced(imprints: readonly ImprintEntry[], index: number, entry: ImprintEntry): ImprintChange | null {
  if (imprints[index] === undefined) return null;
  return { imprints: imprints.map((imprint, position) => (position === index ? entry : imprint)) };
}

/** Свойство отпечатка: `undefined` убирает ключ (пустой `rotation`); значение то же, что было, — `null`: действия нет. */
export function imprintsWithValue(imprints: readonly ImprintEntry[], index: number, key: string, value: unknown): ImprintChange | null {
  const imprint = imprints[index];
  if (imprint === undefined || JSON.stringify(imprint[key]) === JSON.stringify(value)) return null;
  const changed = { ...imprint };
  if (value === undefined) delete changed[key];
  else changed[key] = value;
  return imprintsWithReplaced(imprints, index, changed);
}

/** Копия дописывается в конец со сдвигом на клетку по `x` и выбирается. */
export function imprintsWithCopy(imprints: readonly ImprintEntry[], index: number): ImprintChange | null {
  const imprint = imprints[index];
  return imprint === undefined ? null : { imprints: [...imprints, shiftedImprintCopy(imprint)], selected: imprints.length };
}

/** Отпечаток убран, выбор снят. */
export function imprintsWithout(imprints: readonly ImprintEntry[], index: number): ImprintChange | null {
  return imprints[index] === undefined ? null : { imprints: imprints.filter((_, position) => position !== index), selected: null };
}
