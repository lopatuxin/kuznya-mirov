import { useEffect, useRef, useState } from "react";
import { EditorIcon } from "./EditorIcon";
import { splitJsonTokens } from "./jsonTokens";
import { propertyFieldKind } from "./propertyFieldKind";
import { ENGINE_PROPERTY_NAMES, suggestPropertyNames, type PropertyKind } from "./propertiesDeclarations";
import { parsePropertyValueInput } from "./propertyValueInput";
import { SceneObjectGlyph } from "./SceneObjectGlyph";
import { isSceneColor, type ObjectPropertiesView, type SceneObjectSummary } from "./sceneObjects";

type PropertiesPanelProps = {
  view: ObjectPropertiesView;
  selectedObject: SceneObjectSummary | null;
  /** `scene.json` разобран и движок готов проверять правку — иначе поля показывают, но не пускают в правку. */
  canEdit: boolean;
  imageNames: readonly string[];
  declaredProperties: Readonly<Record<string, PropertyKind>>;
  onSetValue: (key: string, value: unknown) => void;
  onRemove: (key: string) => void;
  onAdd: (key: string, value: unknown) => void;
  onDeclare: (key: string, kind: PropertyKind, value: unknown) => void;
  onCopy: () => void;
  onDelete: () => void;
};

const WIDE_VALUE_LENGTH = 28;
const PROPERTY_KIND_LABELS: Record<PropertyKind, string> = { flag: "флаг", number: "число", time: "время", timer: "таймер" };

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

type EditableTextValueProps = { text: string; canEdit: boolean; onCommit: (rawText: string) => void };

/** Текстовое значение — «Редактор», требование 13: щелчок открывает правку, Enter/уход фокуса — действие, Esc — отмена. */
function EditableTextValue({ text, canEdit, onCommit }: EditableTextValueProps): React.JSX.Element {
  const [isEditing, setIsEditing] = useState(false);
  const [draft, setDraft] = useState(text);

  if (!isEditing) {
    return (
      <span
        className="property-value-text"
        tabIndex={canEdit ? 0 : undefined}
        onClick={() => {
          if (!canEdit) return;
          setDraft(text);
          setIsEditing(true);
        }}
      >
        <JsonValueText text={text} />
      </span>
    );
  }

  function commit(): void {
    setIsEditing(false);
    if (draft !== text) onCommit(draft);
  }

  return (
    <input
      autoFocus
      className="property-value-input"
      value={draft}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === "Enter") event.currentTarget.blur();
        else if (event.key === "Escape") setIsEditing(false);
      }}
    />
  );
}

type ColorPickerInputProps = { value: string; onCommit: (hex: string) => void };

/**
 * Палитра браузера — «Редактор», требование 12: действие при закрытии палитры (`change`), не при
 * каждом движении внутри неё (`input`, который и слушает React `onChange` у обычных полей).
 */
function ColorPickerInput({ value, onCommit }: ColorPickerInputProps): React.JSX.Element {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const input = inputRef.current;
    if (!input) return;
    function handleChange(): void {
      if (input) onCommit(input.value);
    }
    input.addEventListener("change", handleChange);
    return () => input.removeEventListener("change", handleChange);
  }, [onCommit]);

  return <input ref={inputRef} type="color" className="property-color-picker" defaultValue={value} />;
}

type PropertyValueControlProps = {
  propertyKey: string;
  value: unknown;
  valueText: string;
  canEdit: boolean;
  imageNames: readonly string[];
  declaredProperties: Readonly<Record<string, PropertyKind>>;
  onSetValue: (value: unknown) => void;
};

/** Вид поля по свойству — «Редактор», требование 11. */
function PropertyValueControl({ propertyKey, value, valueText, canEdit, imageNames, declaredProperties, onSetValue }: PropertyValueControlProps): React.JSX.Element {
  const kind = propertyFieldKind(propertyKey, value, declaredProperties, imageNames);

  if (kind.kind === "checkbox") {
    return (
      <input
        type="checkbox"
        checked={value as boolean}
        disabled={!canEdit}
        onChange={(event) => onSetValue(event.target.checked)}
      />
    );
  }

  if (kind.kind === "select") {
    return (
      <select
        className="property-value-select"
        value={String(value)}
        disabled={!canEdit}
        onChange={(event) => {
          const option = kind.options.find((candidate) => String(candidate) === event.target.value);
          onSetValue(option ?? event.target.value);
        }}
      >
        {kind.options.map((option) => (
          <option key={String(option)} value={String(option)}>
            {String(option)}
          </option>
        ))}
      </select>
    );
  }

  if (kind.kind === "color") {
    return (
      <span className="property-color-field">
        <ColorPickerInput key={valueText} value={value as string} onCommit={onSetValue} />
        <EditableTextValue text={valueText} canEdit={canEdit} onCommit={(rawText) => onSetValue(parsePropertyValueInput(rawText))} />
      </span>
    );
  }

  return <EditableTextValue text={valueText} canEdit={canEdit} onCommit={(rawText) => onSetValue(parsePropertyValueInput(rawText))} />;
}

type AddPropertyRowProps = {
  existingKeys: readonly string[];
  declaredProperties: Readonly<Record<string, PropertyKind>>;
  onAdd: (key: string, value: unknown) => void;
  onDeclare: (key: string, kind: PropertyKind, value: unknown) => void;
};

/** Строка «+ свойство» — «Редактор», требования 15–16. */
function AddPropertyRow({ existingKeys, declaredProperties, onAdd, onDeclare }: AddPropertyRowProps): React.JSX.Element {
  const [name, setName] = useState("");
  const [valueText, setValueText] = useState("");
  const [kind, setKind] = useState<PropertyKind | "">("");
  const [error, setError] = useState<string | null>(null);

  const trimmedName = name.trim();
  const isKnown = trimmedName !== "" && ((ENGINE_PROPERTY_NAMES as readonly string[]).includes(trimmedName) || declaredProperties[trimmedName] !== undefined);
  const needsKind = trimmedName !== "" && !isKnown && !existingKeys.includes(trimmedName);
  const suggestions = suggestPropertyNames(existingKeys, declaredProperties);

  function reset(): void {
    setName("");
    setValueText("");
    setKind("");
    setError(null);
    // Требование 19/24: свойство уже записано, а Ctrl+Z сразу после Enter должен его отменить —
    // фокус, оставшийся в опустевшем поле «значение», не должен глушить глобальную клавишу.
    (document.activeElement as HTMLElement | null)?.blur();
  }

  function submit(): void {
    if (trimmedName === "") return;
    if (existingKeys.includes(trimmedName)) {
      setError("Свойство уже есть у объекта");
      return;
    }
    if (isKnown) {
      onAdd(trimmedName, parsePropertyValueInput(valueText));
      reset();
      return;
    }
    if (kind === "") {
      setError("Выберите вид свойства");
      return;
    }
    if (kind !== "flag" && valueText.trim() === "") {
      setError("Введите значение");
      return;
    }
    onDeclare(trimmedName, kind, kind === "flag" ? true : parsePropertyValueInput(valueText));
    reset();
  }

  return (
    <div className="add-property-row">
      <div className="add-property-row__fields">
        <input
          className="add-property-row__name"
          list="editor-property-name-suggestions"
          placeholder="имя"
          value={name}
          onChange={(event) => {
            setName(event.target.value);
            setError(null);
          }}
          onKeyDown={(event) => event.key === "Enter" && submit()}
        />
        <datalist id="editor-property-name-suggestions">
          {suggestions.map((suggestion) => (
            <option key={suggestion} value={suggestion} />
          ))}
        </datalist>
        {kind !== "flag" && (
          <input
            className="add-property-row__value"
            placeholder="значение"
            value={valueText}
            onChange={(event) => {
              setValueText(event.target.value);
              setError(null);
            }}
            onKeyDown={(event) => event.key === "Enter" && submit()}
          />
        )}
        {needsKind && (
          <select className="add-property-row__kind" value={kind} onChange={(event) => setKind(event.target.value as PropertyKind)}>
            <option value="" disabled>
              вид…
            </option>
            {(Object.keys(PROPERTY_KIND_LABELS) as PropertyKind[]).map((option) => (
              <option key={option} value={option}>
                {PROPERTY_KIND_LABELS[option]}
              </option>
            ))}
          </select>
        )}
        <button type="button" className="editor-button editor-button--outline" onClick={submit}>
          <EditorIcon name="plus" size={14} />
          Свойство
        </button>
      </div>
      {error !== null && <div className="add-property-row__error">{error}</div>}
    </div>
  );
}

/**
 * Свойства выбранного объекта — «Редактор», требования 11–19: вид поля по свойству, кнопка × у
 * строки, «+ свойство» внизу, «Копия» и «Удалить» в шапке. Значение показывается так, как записано
 * в файле; элемент `objects`, который не объект, — одной строкой его JSON, без правки.
 */
export function PropertiesPanel({
  view,
  selectedObject,
  canEdit,
  imageNames,
  declaredProperties,
  onSetValue,
  onRemove,
  onAdd,
  onDeclare,
  onCopy,
  onDelete,
}: PropertiesPanelProps): React.JSX.Element {
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
        <div className="properties-panel__actions">
          <button type="button" className="editor-button" title="Копия (Ctrl+D)" disabled={!canEdit} onClick={onCopy}>
            <EditorIcon name="copy" size={14} />
          </button>
          <button type="button" className="editor-button" title="Удалить (Delete)" disabled={!canEdit} onClick={onDelete}>
            <EditorIcon name="trash" size={14} />
          </button>
        </div>
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
          <>
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
                    <PropertyValueControl
                      propertyKey={property.key}
                      value={property.value}
                      valueText={property.valueText}
                      canEdit={canEdit}
                      imageNames={imageNames}
                      declaredProperties={declaredProperties}
                      onSetValue={(value) => onSetValue(property.key, value)}
                    />
                    <button type="button" className="property-row__remove" title="Убрать свойство" onClick={() => onRemove(property.key)}>
                      <EditorIcon name="close" size={12} />
                    </button>
                  </dd>
                </div>
              ))}
            </dl>
            {canEdit && (
              <AddPropertyRow
                existingKeys={view.properties.map((property) => property.key)}
                declaredProperties={declaredProperties}
                onAdd={onAdd}
                onDeclare={onDeclare}
              />
            )}
          </>
        )}
      </div>
    </div>
  );
}
