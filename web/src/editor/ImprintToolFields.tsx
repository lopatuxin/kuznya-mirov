import { roundToHundredths } from "./terrainFile";
import { BrushNumberField } from "./TerrainBrushFields";

export type ImprintToolFieldsProps = {
  stampNames: readonly string[];
  stamp: string;
  width: number;
  height: number;
  onStampChange: (stamp: string) => void;
  onWidthChange: (width: number) => void;
  onHeightChange: (height: number) => void;
};

const HEIGHT_HINT = "Меньше нуля — вдавливает";

/** Число больше нуля до сотых; не число и ноль после округления возвращают прежнее значение. */
export function positiveHundredths(previous: number): (typed: number) => number {
  return (typed) => {
    const rounded = roundToHundredths(typed);
    return rounded > 0 ? rounded : previous;
  };
}

/** Число любого знака, кроме нуля, до сотых; ноль после округления возвращает прежнее значение. */
export function nonZeroHundredths(previous: number): (typed: number) => number {
  return (typed) => {
    const rounded = roundToHundredths(typed);
    return rounded !== 0 ? rounded : previous;
  };
}

/** Поля кнопки «Отпечаток» в полосе над сценой — «Правка сцены», требование 22: штамп, ширина и высота нового отпечатка, меньше нуля — вдавливает. */
export function ImprintToolFields({ stampNames, stamp, width, height, onStampChange, onWidthChange, onHeightChange }: ImprintToolFieldsProps): React.JSX.Element {
  return (
    <div className="scene-view__panel scene-view__panel--fields" role="group" aria-label="Отпечаток">
      <span className="scene-view__panel-caption">Отпечаток</span>
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
      <BrushNumberField label="Высота" value={height} hint={HEIGHT_HINT} normalize={nonZeroHundredths(height)} onCommit={onHeightChange} />
    </div>
  );
}
