import { useEffect, useRef, useState } from "react";
import { EditorIcon } from "./EditorIcon";
import { SceneObjectGlyph } from "./SceneObjectGlyph";
import type { SceneObjectSummary } from "./sceneObjects";

type ObjectListProps = {
  objects: SceneObjectSummary[];
  selectedIndex: number | null;
  onSelect: (index: number | null) => void;
  /** «Объектов нет» вне партии; партия и повтор без мира показывают «Мира нет» — требование 13. */
  emptyLabel?: string;
};

function matchesObjectQuery(object: SceneObjectSummary, query: string): boolean {
  return String(object.index) === query || (object.name ?? "").toLowerCase().includes(query);
}

function objectRowClassName(object: SceneObjectSummary, isSelected: boolean): string {
  return [
    "object-row",
    isSelected ? "object-row--selected" : "",
    object.isOnScene ? "" : "object-row--off-scene",
  ].join(" ");
}

type ObjectRowProps = {
  object: SceneObjectSummary;
  isSelected: boolean;
  rowRef: React.Ref<HTMLDivElement> | null;
  onSelect: (index: number) => void;
};

function ObjectRow({ object, isSelected, rowRef, onSelect }: ObjectRowProps): React.JSX.Element {
  return (
    <div
      id={`scene-object-${object.index}`}
      role="option"
      aria-selected={isSelected}
      ref={rowRef}
      className={objectRowClassName(object, isSelected)}
      title={object.isOnScene ? undefined : "Нет на сцене: у объекта нет position и size"}
      onClick={() => onSelect(object.index)}
    >
      <span className="object-row__index">{object.index}</span>
      <SceneObjectGlyph object={object} />
      {object.name !== null ? (
        <span className="object-row__name">{object.name}</span>
      ) : (
        <span className="object-row__name object-row__name--unnamed">без имени</span>
      )}
      {object.name === null && object.image !== null && <span className="object-row__hint">{object.image}</span>}
    </div>
  );
}

/**
 * Список объектов сцены — «Редактор», требования 25 и 27–28: строка — номер и `name`, выбранный на
 * сцене объект подсвечен и прокручен в видимую часть. Поиск по имени или номеру сужает список;
 * стрелки вверх и вниз двигают выбор по видимым строкам, Escape очищает поиск, а потом снимает выбор.
 */
export function ObjectList({ objects, selectedIndex, onSelect, emptyLabel = "Объектов нет" }: ObjectListProps): React.JSX.Element {
  const [query, setQuery] = useState("");
  const selectedRowRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    selectedRowRef.current?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  const normalizedQuery = query.trim().toLowerCase();
  const visibleObjects = normalizedQuery === "" ? objects : objects.filter((object) => matchesObjectQuery(object, normalizedQuery));
  const isSelectedVisible = visibleObjects.some((object) => object.index === selectedIndex);

  function clearQuery(): void {
    setQuery("");
    searchInputRef.current?.focus();
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>): void {
    if (event.shiftKey || event.altKey || event.ctrlKey || event.metaKey) return;
    if (event.key === "Escape") {
      if (query !== "") clearQuery();
      else onSelect(null);
      return;
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    if (visibleObjects.length === 0) return;
    const step = event.key === "ArrowDown" ? 1 : -1;
    const position = visibleObjects.findIndex((object) => object.index === selectedIndex);
    const startPosition = step === 1 ? -1 : visibleObjects.length;
    const nextPosition = Math.min(visibleObjects.length - 1, Math.max(0, (position === -1 ? startPosition : position) + step));
    onSelect(visibleObjects[nextPosition]?.index ?? null);
  }

  return (
    <div className="object-list" onKeyDown={handleKeyDown}>
      <div className="editor-panel-header">
        Объекты
        <span className="editor-count">{objects.length}</span>
      </div>

      {objects.length > 0 && (
        <div className="editor-search" onClick={() => searchInputRef.current?.focus()}>
          <EditorIcon name="search" size={14} />
          <input
            ref={searchInputRef}
            type="search"
            value={query}
            placeholder="Имя или номер"
            aria-label="Найти объект"
            onChange={(event) => setQuery(event.target.value)}
          />
          {query !== "" && (
            <button type="button" className="editor-search__clear" aria-label="Очистить поиск" onClick={clearQuery}>
              <EditorIcon name="close" size={12} />
            </button>
          )}
        </div>
      )}

      {objects.length === 0 && <div className="object-list__empty">{emptyLabel}</div>}
      {objects.length > 0 && visibleObjects.length === 0 && <div className="object-list__empty">Ничего не найдено</div>}
      {visibleObjects.length > 0 && (
        <div
          className="object-list__rows"
          role="listbox"
          tabIndex={0}
          aria-label="Объекты сцены"
          aria-activedescendant={isSelectedVisible ? `scene-object-${selectedIndex}` : undefined}
        >
          {visibleObjects.map((object) => {
            const isSelected = object.index === selectedIndex;
            return (
              <ObjectRow
                key={object.index}
                object={object}
                isSelected={isSelected}
                rowRef={isSelected ? selectedRowRef : null}
                onSelect={onSelect}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}
