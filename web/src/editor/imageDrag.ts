/** Тип данных перетаскивания картинки из вкладки «Картинки»: имя картинки; других перетаскиваний сцена не принимает. */
export const IMAGE_DRAG_TYPE = "application/x-kuznya-image";

/** Что показывает указатель над сценой: картинку здесь можно отпустить (`copy`) или нет (`none`) — «Редактор», требование 25. */
export function resolveImageDropEffect(dragTypes: readonly string[], isAcceptingImages: boolean): "copy" | "none" | null {
  if (!dragTypes.includes(IMAGE_DRAG_TYPE)) return null;
  return isAcceptingImages ? "copy" : "none";
}
