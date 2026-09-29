import { EditorIcon, type EditorIconName } from "./EditorIcon";
import type { HandleMode } from "./handleGeometry";

type HandleModeToolbarProps = {
  mode: HandleMode;
  onChange: (mode: HandleMode) => void;
};

const MODE_BUTTONS: { mode: HandleMode; icon: EditorIconName; title: string }[] = [
  { mode: "translate", icon: "move", title: "Перенос (W)" },
  { mode: "rotate", icon: "rotate", title: "Поворот (E)" },
  { mode: "scale", icon: "scale", title: "Масштаб (R)" },
];

/** Виды ручек над сценой — «Редактор», требование 10: те же режимы, что клавиши `W`, `E`, `R`. */
export function HandleModeToolbar({ mode, onChange }: HandleModeToolbarProps): React.JSX.Element {
  return (
    <div className="scene-view__tools" role="group" aria-label="Вид ручек">
      {MODE_BUTTONS.map((button) => (
        <button
          key={button.mode}
          type="button"
          className={`editor-button scene-view__tool${mode === button.mode ? " scene-view__tool--active" : ""}`}
          title={button.title}
          aria-label={button.title}
          aria-pressed={mode === button.mode}
          // Кнопка не забирает фокус у сцены: `W`, `E`, `R` и `F` работают, пока фокус на ней.
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => onChange(button.mode)}
        >
          <EditorIcon name={button.icon} size={14} />
        </button>
      ))}
    </div>
  );
}
