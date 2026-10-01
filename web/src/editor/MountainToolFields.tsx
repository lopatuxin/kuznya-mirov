import { roundToHundredths } from "./terrainFile";
import { BrushNumberField } from "./TerrainBrushFields";

export type MountainToolFieldsProps = {
  stampNames: readonly string[];
  stamp: string;
  width: number;
  height: number;
  onStampChange: (stamp: string) => void;
  onWidthChange: (width: number) => void;
  onHeightChange: (height: number) => void;
};

/** Число больше нуля до сотых; не число и ноль после округления возвращают прежнее значение. */
function positiveHundredths(previous: number): (typed: number) => number {
  return (typed) => {
    const rounded = roundToHundredths(typed);
    return rounded > 0 ? rounded : previous;
  };
}

/** Поля кнопки «Гора» в полосе над сценой — «Правка сцены», требование 22: штамп, ширина и высота новой горы. */
export function MountainToolFields({ stampNames, stamp, width, height, onStampChange, onWidthChange, onHeightChange }: MountainToolFieldsProps): React.JSX.Element {
  return (
    <div className="scene-view__panel scene-view__panel--fields" role="group" aria-label="Гора">
      <span className="scene-view__panel-caption">Гора</span>
      <label className="scene-view__field">
        Штамп
        <select className="scene-view__field-input scene-view__field-input--select" value={stamp} onChange={(event) => onStampChange(event.target.value)}>
          {stampNames.map((name) => (
            <option key={name} value={name}>
              {name}
            </option>
          ))}
        </select>
      </label>
      <BrushNumberField label="Ширина" value={width} normalize={positiveHundredths(width)} onCommit={onWidthChange} />
      <BrushNumberField label="Высота" value={height} normalize={positiveHundredths(height)} onCommit={onHeightChange} />
    </div>
  );
}
