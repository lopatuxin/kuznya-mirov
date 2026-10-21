import { ColorPickerInput } from "./PropertiesPanel";
import { BrushNumberField } from "./BrushSizeFields";
import { DEFAULT_WATER, hasSea, roundToHundredths, waterWithOption, waterWithSea, type TerrainWater } from "./terrainFile";
import { keepSceneFocus } from "./ToolMenu";

type WaterFieldsProps = {
  /** Вода из рельефа; `null` — воды нет: галочка снята, цвет, прозрачность и рябь неактивны. */
  water: TerrainWater | null;
  onWaterChange: (water: TerrainWater | null) => void;
};

/** Что движок берёт, когда `clarity` или `ripple` в файле нет, — «Рельеф», «Вода». */
const DEFAULT_CLARITY_AND_RIPPLE = "1";

const SMALLEST_CLARITY_AND_RIPPLE = 0.01;

/** Прозрачность и рябь — числа больше нуля до сотых. */
function positiveHundredths(typed: number): number {
  return Math.max(SMALLEST_CLARITY_AND_RIPPLE, roundToHundredths(typed));
}

/**
 * Вода рельефа в окошке группы «Вода» — «Редактор», «Правка сцены»: настройка мира, а не инструмента, поэтому
 * открывается при любом инструменте и не стоит среди полей кисти. Галочка «Море» снимает только `level`, озёра и реки файла остаются.
 */
export function WaterFields({ water, onWaterChange }: WaterFieldsProps): React.JSX.Element {
  const shownWater: TerrainWater = water ?? DEFAULT_WATER;
  const isSea = hasSea(water);
  return (
    <div className="tool-menu__fields" role="group" aria-label="Вода">
      <label className="tool-menu__toggle-field" onMouseDown={keepSceneFocus}>
        <input type="checkbox" checked={isSea} onChange={(event) => onWaterChange(waterWithSea(water, event.target.checked))} />
        Море
      </label>
      <BrushNumberField
        label="Уровень моря"
        value={shownWater.level ?? DEFAULT_WATER.level}
        isDisabled={!isSea}
        normalize={roundToHundredths}
        onCommit={(level) => onWaterChange({ ...shownWater, level })}
      />
      <label className="tool-menu__field">
        Цвет
        <ColorPickerInput key={shownWater.color} value={shownWater.color} isDisabled={water === null} onCommit={(color) => onWaterChange({ ...shownWater, color })} />
      </label>
      <BrushNumberField
        label="Дно видно до глубины, клеток"
        value={shownWater.clarity ?? null}
        placeholder={DEFAULT_CLARITY_AND_RIPPLE}
        isDisabled={water === null}
        normalize={positiveHundredths}
        onCommit={(clarity) => onWaterChange(waterWithOption(shownWater, "clarity", clarity))}
        onClear={() => onWaterChange(waterWithOption(shownWater, "clarity", null))}
      />
      <BrushNumberField
        label="Длина ряби, клеток"
        value={shownWater.ripple ?? null}
        placeholder={DEFAULT_CLARITY_AND_RIPPLE}
        isDisabled={water === null}
        normalize={positiveHundredths}
        onCommit={(ripple) => onWaterChange(waterWithOption(shownWater, "ripple", ripple))}
        onClear={() => onWaterChange(waterWithOption(shownWater, "ripple", null))}
      />
    </div>
  );
}
