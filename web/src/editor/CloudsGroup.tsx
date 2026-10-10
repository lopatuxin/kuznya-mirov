import { useEffect, useRef } from "react";
import { CloudImagesField, type CloudImages } from "./CloudImagesField";
import { DensityField, type ParticleEditing } from "./ParticleFieldControls";

/** Обратные вызовы правки облаков: те же, что у вкладки «Эффекты», без свойств и запрета — их знает колонка «Свойства». */
export type CloudsHandlers = Omit<ParticleEditing, "properties" | "isDisabled">;

type CloudsGroupProps = {
  /** Свойства выбранного объекта: из текста сцены вне партии, из живого мира в ней. */
  properties: Readonly<Record<string, unknown>>;
  /** Поля и кнопки неактивны: повтор или проект без правки. */
  isEditable: boolean;
  images: CloudImages;
  handlers: CloudsHandlers;
};

/** Свойства облаков: их показывает и правит группа, а в общем списке свойств строк для них нет (требование 35). */
export const CLOUD_PROPERTY_NAMES: readonly string[] = ["clouds", "cloud_images"];
const FIRST_CLOUDS_AMOUNT = 0.3;
const CLOUD_LABELS = { clouds: "сколько облаков", cloud_images: "картинки облаков" } as const;

/** Ошибка движка про облака без английских ключей — требование 36: ключ заменён русской подписью поля группы. */
function translateCloudKeys(message: string | undefined): string | undefined {
  return message?.replace(/\b(clouds|cloud_images)\b/g, (key) => CLOUD_LABELS[key as keyof typeof CLOUD_LABELS]);
}

/**
 * Группа «Облака» у выбранного неба — «Редактор», фаза 34, требования 31–37: у объекта с `position`, `size` и `repeat_x`
 * одна кнопка «Добавить облака», а с `clouds` — ползунок «сколько облаков», картинки облаков и «Убрать».
 */
export function CloudsGroup({ properties, isEditable, images, handlers }: CloudsGroupProps): React.JSX.Element | null {
  const isGestureActiveRef = useRef(false);
  const gestureChangeRef = useRef(handlers.onGestureActiveChange);
  gestureChangeRef.current = handlers.onGestureActiveChange;
  // Панель пересоздаётся при смене объекта и партии: жест, который не дошёл до отпускания, не должен оставить перезагрузку закрытой.
  useEffect(
    () => () => {
      if (isGestureActiveRef.current) gestureChangeRef.current(false);
    },
    [],
  );

  // `repeat_x: false` движок считает отсутствующим, и облака у такого объекта — ошибка загрузки («Проверка перед запуском», требование 27).
  if (properties.position === undefined || properties.size === undefined || properties.repeat_x !== true) return null;
  const editing: ParticleEditing = {
    ...handlers,
    properties,
    isDisabled: !isEditable,
    onPreview: (key, value) => translateCloudKeys(handlers.onPreview(key, value)),
    onGestureActiveChange: (isActive) => {
      isGestureActiveRef.current = isActive;
      handlers.onGestureActiveChange(isActive);
    },
  };
  const hasClouds = typeof properties.clouds === "number";

  return (
    <section className="particles-group clouds-group" aria-label="Облака">
      <div className="particles-group__head">
        <h3 className="particles-group__title">Облака</h3>
        {hasClouds && (
          <button
            type="button"
            className="editor-button particles-group__remove"
            disabled={editing.isDisabled}
            title="Снять с неба облака вместе с настройками"
            onClick={() => editing.onRemove(CLOUD_PROPERTY_NAMES.filter((key) => key in properties))}
          >
            Убрать
          </button>
        )}
      </div>
      {hasClouds ? (
        <div className="particles-group__rows">
          <DensityField title={CLOUD_LABELS.clouds} propertyKey="clouds" endLabels={["редкие", "небо в облаках"]} editing={editing} />
          <CloudImagesField editing={editing} images={images} />
        </div>
      ) : (
        <button type="button" className="editor-button" disabled={editing.isDisabled} onClick={() => editing.onCommit("clouds", FIRST_CLOUDS_AMOUNT, undefined)}>
          Добавить облака
        </button>
      )}
    </section>
  );
}
