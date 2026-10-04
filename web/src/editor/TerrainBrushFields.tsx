import { useState } from "react";
import { ColorPickerInput } from "./PropertiesPanel";
import { BRUSH_SIZE_LIMITS, BRUSH_STRENGTH_LIMITS, clampToLimits, parseNumberField, type BrushLimits } from "./terrainBrush";
import { DEFAULT_WATER, roundToHundredths, type TerrainWater } from "./terrainFile";

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
    <label className="scene-view__field" title={hint}>
      {label}
      <input
        type="text"
        inputMode="decimal"
        className="scene-view__field-input"
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

type TerrainBrushFieldsProps = {
  size: number;
  strength: number;
  onSizeChange: (size: number) => void;
  onStrengthChange: (strength: number) => void;
  /** Вода из рельефа; `null` — воды нет: галочка снята, уровень и цвет неактивны. */
  water: TerrainWater | null;
  onWaterChange: (water: TerrainWater | null) => void;
};

export function clampedTo(limits: BrushLimits): (typed: number) => number {
  return (typed) => clampToLimits(typed, limits);
}

/** Поля кисти в полосе над сценой — «Кисти рельефа», требования 2–3, 23: размер, сила и вода рельефа. */
export function TerrainBrushFields({ size, strength, onSizeChange, onStrengthChange, water, onWaterChange }: TerrainBrushFieldsProps): React.JSX.Element {
  const shownWater = water ?? DEFAULT_WATER;
  return (
    <div className="scene-view__tool-row">
      <div className="scene-view__panel scene-view__panel--fields" role="group" aria-label="Кисть">
        <span className="scene-view__panel-caption">Кисть</span>
        <BrushNumberField label="Размер" value={size} normalize={clampedTo(BRUSH_SIZE_LIMITS)} onCommit={onSizeChange} />
        <BrushNumberField label="Сила" value={strength} normalize={clampedTo(BRUSH_STRENGTH_LIMITS)} onCommit={onStrengthChange} />
      </div>
      <div className="scene-view__panel scene-view__panel--fields" role="group" aria-label="Вода">
        {/* Галочка — подпись группы; она не забирает фокус у сцены — клавиши сцены работают и после неё. */}
        <label className="scene-view__panel-caption scene-view__panel-caption--toggle" onMouseDown={(event) => event.preventDefault()}>
          <input type="checkbox" checked={water !== null} onChange={(event) => onWaterChange(event.target.checked ? DEFAULT_WATER : null)} />
          Вода
        </label>
        <BrushNumberField
          label="Уровень"
          value={shownWater.level}
          isDisabled={water === null}
          normalize={roundToHundredths}
          onCommit={(level) => onWaterChange({ ...shownWater, level })}
        />
        <label className="scene-view__field">
          Цвет
          <ColorPickerInput
            key={shownWater.color}
            value={shownWater.color}
            isDisabled={water === null}
            onCommit={(color) => onWaterChange({ ...shownWater, color })}
          />
        </label>
      </div>
    </div>
  );
}
