import { useState } from "react";
import { BrushNumberField } from "./BrushSizeFields";
import type { SceneWind } from "./sceneWind";
import { roundToHundredths } from "./terrainFile";

type WindFieldsProps = {
  wind: SceneWind;
  /** Принимает новый ветер; возвращает текст ошибки движка, если ветер не принят. */
  onWindChange: (wind: SceneWind) => string | undefined;
};

/** Ветер сцены в окошке «Ветер» — «Правка сцены», требование 30: клеток в секунду по каждой оси, числа со знаком до сотой. */
export function WindFields({ wind, onWindChange }: WindFieldsProps): React.JSX.Element {
  const [error, setError] = useState<string | null>(null);
  const [shownWind, setShownWind] = useState(wind);
  // Ветер поменялся снаружи — отмена, перечитывание файла, перезапуск партии с экрана, событие ветра записи: ошибка про прежнее значение уже ни о чём.
  if (shownWind[0] !== wind[0] || shownWind[1] !== wind[1]) {
    setShownWind(wind);
    setError(null);
  }
  const commit = (next: SceneWind): void => setError(onWindChange(next) ?? null);
  return (
    <div className="tool-menu__fields" role="group" aria-label="Ветер">
      <BrushNumberField label="По x" hint="клеток в секунду, вправо" value={wind[0]} normalize={roundToHundredths} onCommit={(x) => commit([x, wind[1]])} />
      <BrushNumberField label="По y" hint="клеток в секунду, вниз" value={wind[1]} normalize={roundToHundredths} onCommit={(y) => commit([wind[0], y])} />
      {error !== null && (
        <div className="tool-menu__error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}
