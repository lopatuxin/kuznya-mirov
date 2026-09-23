import { useEffect, useRef } from "react";
import type { SceneObjectSummary } from "./sceneObjects";

type ObjectListProps = {
  objects: SceneObjectSummary[];
  selectedIndex: number | null;
  onSelect: (index: number) => void;
};

/**
 * Список объектов сцены — «Редактор», требования 25 и 27–28: строка — номер и `name`, выбранный на
 * сцене объект подсвечен и прокручен в видимую часть.
 */
export function ObjectList({ objects, selectedIndex, onSelect }: ObjectListProps): React.JSX.Element {
  const selectedRowRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    selectedRowRef.current?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  if (objects.length === 0) {
    return <div className="object-list object-list--empty">Объектов нет</div>;
  }

  return (
    <div className="object-list">
      {objects.map((object) => {
        const isSelected = object.index === selectedIndex;
        return (
          <button
            key={object.index}
            type="button"
            ref={isSelected ? selectedRowRef : null}
            className={isSelected ? "object-list__row object-list__row--selected" : "object-list__row"}
            onClick={() => onSelect(object.index)}
          >
            {object.index}
            {object.name !== null ? `: ${object.name}` : ""}
          </button>
        );
      })}
    </div>
  );
}
