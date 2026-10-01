import { useState } from "react";
import { EditorIcon } from "./EditorIcon";
import { parsePropertyValueInput } from "./propertyValueInput";
import type { MountainEntry } from "./terrainFile";

type MountainPropertiesPanelProps = {
  /** Номер горы в `stamps` с нуля — в заголовке он с единицы. */
  index: number;
  entry: MountainEntry;
  stampNames: readonly string[];
  /** Файлы проекта разобраны и движок готов проверять правку. */
  canEdit: boolean;
  /** `undefined` убирает ключ: так пустое `rotation` уходит из файла. */
  onSetValue: (key: string, value: unknown) => void;
  onCopy: () => void;
  onDelete: () => void;
};

const VALUE_KEYS = ["position", "size", "height", "rotation"] as const;

type MountainValueFieldProps = { value: string; placeholder?: string; canEdit: boolean; onCommit: (text: string) => void };

/** Значение правится на месте, как текстовое свойство: Enter или уход из поля принимает, Esc возвращает прежнее. */
function MountainValueField({ value, placeholder, canEdit, onCommit }: MountainValueFieldProps): React.JSX.Element {
  const [draft, setDraft] = useState<string | null>(null);

  function commit(): void {
    const typed = draft;
    setDraft(null);
    if (typed !== null && typed !== value) onCommit(typed);
  }

  return (
    <input
      className="property-value-input"
      value={draft ?? value}
      placeholder={placeholder}
      disabled={!canEdit}
      onFocus={() => setDraft(value)}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === "Enter") event.currentTarget.blur();
        else if (event.key === "Escape") setDraft(null);
      }}
    />
  );
}

/**
 * Свойства выбранной горы — «Редактор», «Правка сцены», требование 30: заголовок «Гора N», `stamp` —
 * выпадающий список штампов, остальные поля — текстом JSON; «Копия» и «Удалить» в шапке. Набранное
 * пишется как есть — ошибку называет движок, файл при ошибке не пишется.
 */
export function MountainPropertiesPanel({ index, entry, stampNames, canEdit, onSetValue, onCopy, onDelete }: MountainPropertiesPanelProps): React.JSX.Element {
  const stamp = typeof entry.stamp === "string" ? entry.stamp : "";
  const stampOptions = stampNames.includes(stamp) ? stampNames : [stamp, ...stampNames];

  function commitValue(key: (typeof VALUE_KEYS)[number], text: string): void {
    if (text.trim() !== "") onSetValue(key, parsePropertyValueInput(text));
    else if (key === "rotation") onSetValue(key, undefined);
  }

  return (
    <div className="properties-panel">
      <div className="editor-panel-header">
        Гора {index + 1}
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
        <dl className="property-list">
          <div className="property-row">
            <dt className="property-row__key">stamp</dt>
            <dd className="property-row__value">
              <select className="property-value-select" value={stamp} disabled={!canEdit} onChange={(event) => onSetValue("stamp", event.target.value)}>
                {stampOptions.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </dd>
          </div>
          {VALUE_KEYS.map((key) => (
            <div key={key} className="property-row">
              <dt className="property-row__key">{key}</dt>
              <dd className="property-row__value">
                <MountainValueField
                  value={entry[key] === undefined ? "" : JSON.stringify(entry[key])}
                  placeholder={key === "rotation" ? "0" : undefined}
                  canEdit={canEdit}
                  onCommit={(text) => commitValue(key, text)}
                />
              </dd>
            </div>
          ))}
        </dl>
      </div>
    </div>
  );
}
