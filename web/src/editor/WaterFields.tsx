import { ColorPickerInput } from "./PropertiesPanel";
import { BrushNumberField } from "./BrushSizeFields";
import { DEFAULT_WATER, roundToHundredths, type TerrainWater } from "./terrainFile";
import { keepSceneFocus } from "./ToolMenu";

type WaterFieldsProps = {
  /** Вода из рельефа; `null` — воды нет: галочка снята, уровень и цвет неактивны. */
  water: TerrainWater | null;
  onWaterChange: (water: TerrainWater | null) => void;
};

/**
 * Вода рельефа в окошке группы «Вода» — «Кисти рельефа», требования 3, 23: настройка мира, а не инструмента, поэтому
 * открывается при любом инструменте и не стоит среди полей кисти.
 */
export function WaterFields({ water, onWaterChange }: WaterFieldsProps): React.JSX.Element {
  const shownWater = water ?? DEFAULT_WATER;
  return (
    <div className="tool-menu__fields" role="group" aria-label="Вода">
      <label className="tool-menu__toggle-field" onMouseDown={keepSceneFocus}>
        <input type="checkbox" checked={water !== null} onChange={(event) => onWaterChange(event.target.checked ? DEFAULT_WATER : null)} />
        Вода в сцене
      </label>
      <BrushNumberField
        label="Уровень"
        value={shownWater.level}
        isDisabled={water === null}
        normalize={roundToHundredths}
        onCommit={(level) => onWaterChange({ ...shownWater, level })}
      />
      <label className="tool-menu__field">
        Цвет
        <ColorPickerInput key={shownWater.color} value={shownWater.color} isDisabled={water === null} onCommit={(color) => onWaterChange({ ...shownWater, color })} />
      </label>
    </div>
  );
}
