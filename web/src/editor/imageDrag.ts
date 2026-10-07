/** Тип данных перетаскивания картинки из вкладки «Картинки»: имя картинки; других перетаскиваний сцена не принимает. */
export const IMAGE_DRAG_TYPE = "application/x-kuznya-image";

/** Тип данных перетаскивания вида из вкладки «Частицы»: имя вида; он падает на сцену, как картинка («Редактор», «Правка сцены», требование 34). */
export const PARTICLES_DRAG_TYPE = "application/x-kuznya-particles";

/** Что показывает указатель над сценой: картинку или вид частиц здесь можно отпустить (`copy`) или нет (`none`) — «Редактор», требование 25. */
export function resolveSceneDropEffect(dragTypes: readonly string[], isAcceptingDrops: boolean): "copy" | "none" | null {
  if (!dragTypes.includes(IMAGE_DRAG_TYPE) && !dragTypes.includes(PARTICLES_DRAG_TYPE)) return null;
  return isAcceptingDrops ? "copy" : "none";
}
