import { MODEL_DRAG_TYPE } from "./imageDrag";
import type { ProjectModel } from "./projectModels";
import { roundToHundredths } from "./terrainFile";

type ModelsPanelProps = { models: readonly ProjectModel[] };

function formatModelSize(size: ProjectModel["size"]): string {
  return size.map((side) => String(roundToHundredths(side))).join(" × ");
}

/**
 * Вкладка «Модели» — «Редактор», «Окно редактора», требование 23 фазы 25: модели `files.models` в порядке объявления, у каждой
 * имя и размер в клетках (ширина, глубина, высота); имя перетаскивают на трёхмерную сцену.
 */
export function ModelsPanel({ models }: ModelsPanelProps): React.JSX.Element {
  if (models.length === 0) {
    return <div className="problems-panel problems-panel--empty">В игре нет моделей</div>;
  }
  return (
    <ul className="images-panel__list">
      {models.map((model) => (
        <li
          key={model.name}
          className="images-panel__item models-panel__item"
          draggable
          onDragStart={(event) => {
            event.dataTransfer.setData(MODEL_DRAG_TYPE, model.name);
            event.dataTransfer.effectAllowed = "copy";
          }}
          title="Перетащите на сцену"
        >
          <span className="images-panel__name">{model.name}</span>
          <span className="models-panel__size">{formatModelSize(model.size)}</span>
        </li>
      ))}
    </ul>
  );
}
