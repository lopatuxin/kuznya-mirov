import type { SceneWind } from "./sceneWind";
import { ToolMenu } from "./ToolMenu";
import { WindFields } from "./WindFields";

type WindMenuProps = {
  wind: SceneWind;
  /** Ветер нельзя править: в проекте ошибки или идёт повтор. */
  isDisabled: boolean;
  onWindChange: (wind: SceneWind) => string | undefined;
};

/** Кнопка «Ветер» с выпадающим окошком в верхней полосе плоской сцены — «Правка сцены», требование 30, как «Вода» в трёхмерной. */
export function WindMenu({ wind, isDisabled, onWindChange }: WindMenuProps): React.JSX.Element {
  return (
    <ToolMenu label="Ветер" caption="Ветер" icon="wind" title={isDisabled ? "Ветер: недоступен — в проекте ошибки или идёт повтор" : "Ветер сцены: скорость по x и y"} isDisabled={isDisabled}>
      <WindFields wind={wind} onWindChange={onWindChange} />
    </ToolMenu>
  );
}
