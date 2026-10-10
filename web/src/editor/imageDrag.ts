/** Тип данных перетаскивания картинки из вкладки «Картинки»: имя картинки; других перетаскиваний сцена не принимает. */
export const IMAGE_DRAG_TYPE = "application/x-kuznya-image";

/** Тип данных перетаскивания карточки из вкладки «Эффекты»: ключ эффекта; она падает на сцену, как картинка («Редактор», «Правка сцены», требования 34–35). */
export const PARTICLES_DRAG_TYPE = "application/x-kuznya-particles";

/**
 * Карточку листьев тянут к объекту, которому достанется листопад («Редактор», требование 35): сцена обводит его рамкой.
 * Данные перетаскивания над сценой браузер не отдаёт, виден только список типов, поэтому карточка ставит ещё и этот тип.
 */
export const LEAVES_DRAG_TYPE = "application/x-kuznya-particles-leaves";

/** Что показывает указатель над сценой: картинку или карточку частиц здесь можно отпустить (`copy`) или нет (`none`) — «Редактор», требование 25. */
export function resolveSceneDropEffect(dragTypes: readonly string[], isAcceptingDrops: boolean): "copy" | "none" | null {
  if (!dragTypes.includes(IMAGE_DRAG_TYPE) && !dragTypes.includes(PARTICLES_DRAG_TYPE)) return null;
  return isAcceptingDrops ? "copy" : "none";
}
