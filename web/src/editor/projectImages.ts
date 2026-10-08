import type { LoadedProjectImage } from "../projectLoader";
import type { Vec2 } from "./objectPlacement";
import type { ProjectImageDescription } from "./projectFiles";

/** Картинка вкладки «Картинки»: описание из `game.json` и разжатые точки; `null` — файл не прочитался или не разжался. */
export type ProjectImageTile = { description: ProjectImageDescription; image: LoadedProjectImage | null };

/** Ключи слоя, которые новый объект берёт у выбранного, — «Редактор», «Правка сцены», требование 26. */
const INHERITED_KEYS = ["layer", "parallax"] as const;

function roundToHundredth(value: number): number {
  return Math.round(value * 100) / 100;
}

/** Картинки `files.images` в порядке объявления, у каждой — её точки, если страница их разжала. */
export function buildImageTiles(descriptions: readonly ProjectImageDescription[], loaded: readonly LoadedProjectImage[]): ProjectImageTile[] {
  return descriptions.map((description) => ({ description, image: loaded.find((image) => image.name === description.name) ?? null }));
}

/**
 * Размер одного кадра в точках файла — «Редактор», требование 27: без `frames` весь файл, с `frames` без `columns` —
 * кадры в одну строку, с `columns` — сеткой в `ceil(frames / columns)` строк.
 */
export function imageFrameSize(image: Pick<LoadedProjectImage, "width" | "height">, description: ProjectImageDescription): Vec2 {
  const { frames, columns } = description;
  if (frames === null) return [image.width, image.height];
  if (columns === null) return [image.width / frames, image.height];
  return [image.width / columns, image.height / Math.ceil(frames / columns)];
}

/**
 * `size` нового объекта — «Редактор», требование 27: свой `size` картинки, иначе кадр в точках, делённый на
 * `cell_pixels`, а без него — высота 1 и ширина по пропорциям кадра. Числа — до сотой клетки.
 */
export function newObjectSize(frame: Vec2, description: ProjectImageDescription, cellPixels: number | null): [number, number] {
  if (description.size !== null) return [roundToHundredth(description.size[0]), roundToHundredth(description.size[1])];
  if (cellPixels !== null) return [roundToHundredth(frame[0] / cellPixels), roundToHundredth(frame[1] / cellPixels)];
  return [roundToHundredth(frame[0] / frame[1]), 1];
}

/** `parallax` слоя, на котором встанет новый объект: у выбранного, иначе 1 — «Редактор», требование 28. */
export function newObjectParallax(selected: Record<string, unknown> | null): number {
  const parallax = selected?.parallax;
  return typeof parallax === "number" && Number.isFinite(parallax) ? parallax : 1;
}

/**
 * Новый источник частиц — «Редактор», требование 34: `position` серединой под указателем, `size` одна клетка, главное
 * свойство эффекта со значением `value` (`smoke` или `sparks` 0,5), за ними `layer` и `parallax`, если они есть у `target` —
 * объекта, на который карточку отпустили, а мимо объектов — выбранного.
 */
export function createParticlesObject(mainKey: string, value: number, middle: Vec2, target: Record<string, unknown> | null): Record<string, unknown> {
  const object: Record<string, unknown> = {
    position: [roundToHundredth(middle[0] - 0.5), roundToHundredth(middle[1] - 0.5)],
    size: [1, 1],
    [mainKey]: value,
  };
  for (const key of INHERITED_KEYS) {
    const value = target?.[key];
    if (value !== undefined) object[key] = value;
  }
  return object;
}

/**
 * Новый объект из картинки — «Редактор», требования 26 и 28: `position`, `size`, `image`, за ними `layer` и `parallax`,
 * если они есть у выбранного; середина объекта — в `middle`, записанном месте под указателем.
 */
export function createImageObject(imageName: string, size: Vec2, middle: Vec2, selected: Record<string, unknown> | null): Record<string, unknown> {
  const object: Record<string, unknown> = {
    position: [roundToHundredth(middle[0] - size[0] / 2), roundToHundredth(middle[1] - size[1] / 2)],
    size: [size[0], size[1]],
    image: imageName,
  };
  for (const key of INHERITED_KEYS) {
    const value = selected?.[key];
    if (value !== undefined) object[key] = value;
  }
  return object;
}
