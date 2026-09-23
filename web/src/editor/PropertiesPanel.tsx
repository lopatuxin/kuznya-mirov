import { EditorIcon } from "./EditorIcon";
import { splitJsonTokens } from "./jsonTokens";
import { SceneObjectGlyph } from "./SceneObjectGlyph";
import { isSceneColor, type ObjectPropertiesView, type SceneObjectSummary } from "./sceneObjects";

type PropertiesPanelProps = { view: ObjectPropertiesView; selectedObject: SceneObjectSummary | null };

const WIDE_VALUE_LENGTH = 28;

/** `"#rrggbb"` в JSON-записи значения — цвет без кавычек, иначе `null`. */
function colorOfJsonValue(jsonText: string): string | null {
  const unquoted = jsonText.slice(1, -1);
  return jsonText.startsWith('"') && jsonText.endsWith('"') && isSceneColor(unquoted) ? unquoted : null;
}

function JsonValueText({ text }: { text: string }): React.JSX.Element {
  const color = colorOfJsonValue(text);
  return (
    <>
      {color !== null && <span className="color-swatch" style={{ background: color }} />}
      {splitJsonTokens(text).map((token, index) => (
        <span key={index} className={`json-token json-token--${token.kind}`}>
          {token.text}
        </span>
      ))}
    </>
  );
}

function SelectedObjectHeader({ object }: { object: SceneObjectSummary }): React.JSX.Element {
  return (
    <div className="selected-object">
      <SceneObjectGlyph object={object} isLarge />
      <div className="selected-object__text">
        <span className={object.name !== null ? "selected-object__name" : "selected-object__name selected-object__name--unnamed"}>
          {object.name ?? "без имени"}
        </span>
        <span className="selected-object__meta">
          объект № {object.index}
          {!object.isOnScene && " · нет на сцене"}
        </span>
      </div>
    </div>
  );
}

/**
 * Свойства выбранного объекта — «Редактор», требования 29–30: по строке на ключ в порядке файла,
 * значение компактным JSON; элемент `objects`, который не объект, — одной строкой его JSON.
 * Короткое значение стоит справа от имени, длинное — под ним во всю ширину панели.
 */
export function PropertiesPanel({ view, selectedObject }: PropertiesPanelProps): React.JSX.Element {
  if (view.status === "none" || selectedObject === null) {
    return (
      <div className="properties-panel">
        <div className="editor-panel-header">Свойства</div>
        <div className="properties-panel__empty">
          <EditorIcon name="pointer" size={22} />
          <span className="properties-panel__empty-title">Объект не выбран</span>
          <span>Щёлкните по объекту на сцене или выберите его в списке</span>
        </div>
      </div>
    );
  }

  return (
    <div className="properties-panel">
      <div className="editor-panel-header">
        Свойства
        {view.status === "object" && <span className="editor-count">{view.properties.length}</span>}
      </div>
      <div className="properties-panel__body">
        <SelectedObjectHeader object={selectedObject} />
        {view.status === "not-object" ? (
          <div className="property-row property-row--wide">
            <span className="property-row__value">
              <JsonValueText text={view.json} />
            </span>
          </div>
        ) : (
          <dl className="property-list">
            {view.properties.map((property) => (
              <div
                key={property.key}
                className={property.valueText.length > WIDE_VALUE_LENGTH ? "property-row property-row--wide" : "property-row"}
              >
                <dt className="property-row__key" title={property.key}>
                  {property.key}
                </dt>
                <dd className="property-row__value">
                  <JsonValueText text={property.valueText} />
                </dd>
              </div>
            ))}
          </dl>
        )}
      </div>
    </div>
  );
}
