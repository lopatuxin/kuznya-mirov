import { useState } from "react";
import { BRUSH_SIZE_LIMITS, BRUSH_STRENGTH_LIMITS, clampToLimits, parseNumberField, type BrushLimits } from "./terrainBrush";

type BrushNumberFieldProps = {
  label: string;
  /** `null` — поле пусто и показывает `placeholder`: значения нет, действует значение по умолчанию. */
  value: number | null;
  isDisabled?: boolean;
  /** Подсказка поля при наведении. */
  hint?: string;
  /** Значение по умолчанию, видимое в пустом поле. */
  placeholder?: string;
  /** Что становится значением набранного числа: прижатое к пределам или округлённое до сотых. */
  normalize: (typed: number) => number;
  onCommit: (value: number) => void;
  /** Что делать, когда стёрли значение и оставили поле пустым; без него пустое поле возвращает прежнее. */
  onClear?: () => void;
};

/** Число правится, как текстовое значение свойства: Enter или уход из поля принимает, Esc и не число возвращают прежнее. */
export function BrushNumberField({ label, value, isDisabled = false, hint, placeholder, normalize, onCommit, onClear }: BrushNumberFieldProps): React.JSX.Element {
  const [draft, setDraft] = useState<string | null>(null);
  const shownValue = value === null ? "" : String(value);

  function commit(): void {
    const text = draft;
    setDraft(null);
    if (text === null) return;
    const typed = parseNumberField(text);
    if (typed === null) {
      if (text.trim() === "" && value !== null) onClear?.();
      return;
    }
    const next = normalize(typed);
    if (next !== value) onCommit(next);
  }

  return (
    <label className="tool-menu__field" title={hint}>
      {label}
      <input
        type="text"
        inputMode="decimal"
        className="tool-menu__field-input"
        value={draft ?? shownValue}
        placeholder={placeholder}
        disabled={isDisabled}
        onFocus={(event) => {
          setDraft(shownValue);
          event.currentTarget.select();
        }}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
          else if (event.key === "Escape") setDraft(null);
        }}
      />
    </label>
  );
}

/** Подсказка поля «Размер»: его меняет и колесо над сценой. */
const BRUSH_SIZE_HINT = "Над сценой — Ctrl+колесо";

type BrushSizeFieldsProps = {
  size: number;
  strength: number;
  onSizeChange: (size: number) => void;
  onStrengthChange: (strength: number) => void;
};

function clampedTo(limits: BrushLimits): (typed: number) => number {
  return (typed) => clampToLimits(typed, limits);
}

/**
 * Размер и сила кисти — «Кисти рельефа», требование 2, и «Покраска», требования 2–3: одни и те же числа у кистей
 * рельефа в окошке «Рельеф» и у кисти материалов в окошке «Материалы».
 */
export function BrushSizeFields({ size, strength, onSizeChange, onStrengthChange }: BrushSizeFieldsProps): React.JSX.Element {
  return (
    <div className="tool-menu__fields" role="group" aria-label="Кисть">
      <BrushNumberField label="Размер" value={size} hint={BRUSH_SIZE_HINT} normalize={clampedTo(BRUSH_SIZE_LIMITS)} onCommit={onSizeChange} />
      <BrushNumberField label="Сила" value={strength} normalize={clampedTo(BRUSH_STRENGTH_LIMITS)} onCommit={onStrengthChange} />
    </div>
  );
}
