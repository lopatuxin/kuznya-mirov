import type { ObjectPropertiesView } from "./sceneObjects";

type PropertiesPanelProps = { view: ObjectPropertiesView };

/**
 * Свойства выбранного объекта — «Редактор», требования 29–30: по строке на ключ в порядке файла,
 * значение компактным JSON; элемент `objects`, который не объект, — одной строкой его JSON.
 */
export function PropertiesPanel({ view }: PropertiesPanelProps): React.JSX.Element {
  if (view.status === "none") {
    return <div className="properties-panel properties-panel--empty">Объект не выбран</div>;
  }

  if (view.status === "not-object") {
    return (
      <div className="properties-panel">
        <div className="properties-panel__row">
          <span className="properties-panel__value">{view.json}</span>
        </div>
      </div>
    );
  }

  return (
    <div className="properties-panel">
      {view.properties.map((property) => (
        <div key={property.key} className="properties-panel__row">
          <span className="properties-panel__key">{property.key}</span>
          <span className="properties-panel__value">{property.valueText}</span>
        </div>
      ))}
    </div>
  );
}
