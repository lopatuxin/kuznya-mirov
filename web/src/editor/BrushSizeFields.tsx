import { useState } from "react";
import { BRUSH_SIZE_LIMITS, BRUSH_STRENGTH_LIMITS, clampToLimits, parseNumberField, type BrushLimits } from "./terrainBrush";

type BrushNumberFieldProps = {
  label: string;
  value: number;
  isDisabled?: boolean;
  /** Подсказка поля при наведении. */
  hint?: string;
  /** Что становится значением набранного числа: прижатое к пределам или округлённое до сотых. */
  normalize: (typed: number) => number;
  onCommit: (value: number) => void;
};

/** Число правится, как текстовое значение свойства: Enter или уход из поля принимает, Esc и не число возвращают прежнее. */
export function BrushNumberField({ label, value, isDisabled = false, hint, normalize, onCommit }: BrushNumberFieldProps): React.JSX.Element {
  const [draft, setDraft] = useState<string | null>(null);

  function commit(): void {
    const typed = draft === null ? null : parseNumberField(draft);
    setDraft(null);
    if (typed === null) return;
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
        value={draft ?? String(value)}
        disabled={isDisabled}
        onFocus={(event) => {
          setDraft(String(value));
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
