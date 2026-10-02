/** Маска покрытия: по байту на точку строками сверху вниз; байт — красный канал файла маски, как его читает движок. */
export type MaskBytes = { width: number; height: number; pixels: Uint8Array };

/** Маски по путям файлов в проекте: `terrain/rock.png` → точки. Байты внутри не меняются никогда — их копируют. */
export type MaskSet = Readonly<Record<string, MaskBytes>>;

export const NO_MASKS: MaskSet = {};

export function areMasksEqual(first: MaskBytes, second: MaskBytes): boolean {
  if (first === second) return true;
  if (first.width !== second.width || first.height !== second.height || first.pixels.length !== second.pixels.length) return false;
  return first.pixels.every((value, index) => value === second.pixels[index]);
}

/** Пути масок из `target`, которых нет в `base` или байты которых другие, — их и надо записать. */
export function changedMaskPaths(base: MaskSet, target: MaskSet): string[] {
  return Object.keys(target).filter((path) => base[path] === undefined || !areMasksEqual(base[path], target[path] as MaskBytes));
}

/** Наборы отличаются: путей разное число, путь другой или байты хоть одной маски. */
export function areMaskSetsEqual(first: MaskSet, second: MaskSet): boolean {
  return Object.keys(first).length === Object.keys(second).length && changedMaskPaths(first, second).length === 0;
}
