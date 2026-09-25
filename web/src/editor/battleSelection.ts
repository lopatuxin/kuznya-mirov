import type { ObjectPropertiesView } from "./sceneObjects";
import type { SceneObjectSummary } from "./sceneObjects";
import type { WorldObjectSummary } from "./battleTypes";

export type LiveSelection = { id: number; generation: number };

/**
 * Выбор снимается, если объекта с этой меткой жизни больше нет в мире — «Редактор», требование 15:
 * выбран объект, а не номер, даже если его номер занял новый объект с другой меткой жизни.
 */
export function resolveLiveSelection(current: LiveSelection | null, worldObjects: readonly WorldObjectSummary[]): LiveSelection | null {
  if (current === null) return null;
  const stillAlive = worldObjects.some((object) => object.id === current.id && object.generation === current.generation);
  return stillAlive ? current : null;
}

/** Живой объект по выбранному номеру, если он ещё жив под той же меткой — для смены выбора кликом. */
export function findWorldObject(worldObjects: readonly WorldObjectSummary[], id: number): WorldObjectSummary | undefined {
  return worldObjects.find((object) => object.id === id);
}

/**
 * Список живых объектов в виде, который понимает `ObjectList` — «Редактор», требование 13. Цвет,
 * картинка и «нет на сцене» файловому списку не нужны в партии — не показываются панелью объектов.
 */
export function buildLiveObjectSummaries(worldObjects: readonly WorldObjectSummary[]): SceneObjectSummary[] {
  return worldObjects.map((object) => ({ index: object.id, name: object.name, color: null, image: null, isOnScene: true }));
}

/**
 * Подпись пустого списка объектов партии и повтора — «Редактор», требование 13: до появления мира
 * (стартовый экран без `world_runs` до `new_game`, или после `quit`) это не «Объектов нет».
 */
export function liveObjectListEmptyLabel(hasWorld: boolean): string {
  return hasWorld ? "Объектов нет" : "Мира нет";
}

/**
 * «Повтор» доступна — «Редактор», требование 28: запись есть, сейчас не сам повтор и не идущая
 * партия, и проект загружен без ошибок. `sceneAvailable` — не только для первого «Запуска»: неудачная
 * перезагрузка проекта во время партии/повтора (`clear_game` движка) роняет саму запись у движка, и
 * без этой проверки «Повтор» оставалась бы нажимаемой и не делала бы ничего.
 */
export function resolveCanStartReplay(hasRecording: boolean, mode: "edit" | "battle" | "replay", isRunning: boolean, sceneAvailable: boolean): boolean {
  return hasRecording && mode !== "replay" && !isRunning && sceneAvailable;
}

/**
 * Свойства живого объекта в виде, который понимает `PropertiesPanel` — «Редактор», требование 14:
 * `object_properties` уже отдаёт значения в единицах файла, здесь они только раскладываются по
 * тому же контракту `ObjectPropertiesView`, что и файловая правка.
 */
export function buildLivePropertiesView(properties: Record<string, unknown> | undefined): ObjectPropertiesView {
  if (properties === undefined) return { status: "none" };
  return {
    status: "object",
    properties: Object.entries(properties).map(([key, value]) => ({ key, value, valueText: JSON.stringify(value) })),
  };
}
