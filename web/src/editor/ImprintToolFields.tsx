import { roundToHundredths } from "./terrainFile";
import { BrushNumberField } from "./BrushSizeFields";
import type { StampPreview } from "./stampPreview";
import { keepSceneFocus } from "./ToolMenu";

export type ImprintToolFieldsProps = {
  stampNames: readonly string[];
  stamp: string;
  /** Картинки штампов по имени; у штампа без картинки карточка — одно название. */
  previews: ReadonlyMap<string, StampPreview>;
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

/**
 * Поля «Отпечатка» в окошке группы «Рельеф» — «Правка сцены», требование 22: штамп карточкой с картинкой, ширина и высота
 * нового отпечатка. Высота меньше нуля вдавливает, и картинки показывают ту же форму впадиной.
 */
export function ImprintToolFields({ stampNames, stamp, previews, width, height, onStampChange, onWidthChange, onHeightChange }: ImprintToolFieldsProps): React.JSX.Element {
  return (
    <div className="tool-menu__fields" role="group" aria-label="Отпечаток">
      <div className="stamp-picker" role="radiogroup" aria-label="Штамп">
        {stampNames.map((name) => {
          const preview = previews.get(name);
          const picture = preview === undefined ? undefined : height < 0 ? preview.lowered : preview.raised;
          const isChosen = name === stamp;
          return (
            <button
              key={name}
              type="button"
              role="radio"
              aria-checked={isChosen}
              className={`stamp-picker__card${isChosen ? " stamp-picker__card--chosen" : ""}`}
              title={name}
              onMouseDown={keepSceneFocus}
              onClick={() => onStampChange(name)}
            >
              {picture !== undefined && <img className="stamp-picker__picture" src={picture} alt="" />}
              <span className="stamp-picker__name">{name}</span>
            </button>
          );
        })}
      </div>
      <BrushNumberField label="Ширина" value={width} normalize={positiveHundredths(width)} onCommit={onWidthChange} />
      <BrushNumberField label="Высота" value={height} hint={HEIGHT_HINT} normalize={nonZeroHundredths(height)} onCommit={onHeightChange} />
    </div>
  );
}
