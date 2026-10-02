import { EditorIcon, type EditorIconName } from "./EditorIcon";
import type { HandleMode } from "./handleGeometry";
import { MountainToolFields, type MountainToolFieldsProps } from "./MountainToolFields";
import { PaintBrushFields, type PaintBrushFieldsProps } from "./PaintBrushFields";
import { TerrainBrushFields } from "./TerrainBrushFields";
import type { BrushKind } from "./terrainBrush";
import type { TerrainWater } from "./terrainFile";

/** Кнопка «Гора» и её поля («Правка сцены», требования 22–23). */
export type MountainToolbar = {
  isSelected: boolean;
  /** Кнопка нажимается, когда в игре объявлены штампы. */
  isEnabled: boolean;
  fields: MountainToolFieldsProps;
  onSelect: () => void;
};

/** Кнопка «Покрасить» и её поля («Покраска», требования 1–4). */
export type PaintToolbar = {
  isSelected: boolean;
  /** Кнопка нажимается, когда в игре объявлены материалы. */
  isEnabled: boolean;
  fields: PaintBrushFieldsProps;
  onSelect: () => void;
};

const NO_STAMPS_TITLE = "Нет штампов: объяви files.stamps в game.json";
const NO_MATERIALS_TITLE = "Нет материалов: объяви files.materials в game.json";

type HandleModeToolbarProps = {
  mode: HandleMode;
  /** Выбранная кисть; `null` — выбраны ручки. Выбран всегда один инструмент. */
  brushKind: BrushKind | null;
  /** Кисти есть только в трёхмерной сцене вне партии («Кисти рельефа», требование 4). */
  areBrushesAvailable: boolean;
  brushSize: number;
  brushStrength: number;
  water: TerrainWater | null;
  mountainTool: MountainToolbar;
  paintTool: PaintToolbar;
  onChange: (mode: HandleMode) => void;
  onBrushChange: (kind: BrushKind) => void;
  onBrushSizeChange: (size: number) => void;
  onBrushStrengthChange: (strength: number) => void;
  onWaterChange: (water: TerrainWater | null) => void;
};

const MODE_BUTTONS: { mode: HandleMode; icon: EditorIconName; title: string }[] = [
  { mode: "translate", icon: "move", title: "Перенос (W)" },
  { mode: "rotate", icon: "rotate", title: "Поворот (E)" },
  { mode: "scale", icon: "scale", title: "Масштаб (R)" },
];

const BRUSH_BUTTONS: { kind: BrushKind; icon: EditorIconName; title: string }[] = [
  { kind: "raise", icon: "terrain-raise", title: "Поднять землю (с Shift — опустить)" },
  { kind: "level", icon: "terrain-level", title: "Выровнять до высоты точки нажатия" },
  { kind: "smooth", icon: "terrain-smooth", title: "Сгладить перепады" },
];

type ToolButtonProps = { title: string; isActive: boolean; isDisabled?: boolean; onClick: () => void; children: React.ReactNode };

function ToolButton({ title, isActive, isDisabled = false, onClick, children }: ToolButtonProps): React.JSX.Element {
  return (
    <button
      type="button"
      className={`editor-button scene-view__tool${isActive ? " scene-view__tool--active" : ""}`}
      title={title}
      aria-label={title}
      aria-pressed={isActive}
      disabled={isDisabled}
      // Кнопка не забирает фокус у сцены: `W`, `E`, `R` и `F` работают, пока фокус на ней.
      onMouseDown={(event) => event.preventDefault()}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

/**
 * Инструменты над сценой — «Редактор», требование 10, и «Кисти рельефа», требования 1–3: виды ручек
 * (те же режимы, что клавиши `W`, `E`, `R`), кисти рельефа, «Гора», «Покрасить» и, пока выбрана кисть, поля размера, силы и воды,
 * пока выбрана «Гора» — поля штампа, ширины и высоты, пока выбрана «Покрасить» — размера, силы и материала.
 */
export function HandleModeToolbar({
  mode,
  brushKind,
  areBrushesAvailable,
  brushSize,
  brushStrength,
  water,
  mountainTool,
  paintTool,
  onChange,
  onBrushChange,
  onBrushSizeChange,
  onBrushStrengthChange,
  onWaterChange,
}: HandleModeToolbarProps): React.JSX.Element {
  return (
    <div className="scene-view__tools">
      <div className="scene-view__tool-row">
        <div className="scene-view__panel" role="group" aria-label="Ручки объекта">
          <span className="scene-view__panel-caption">Объект</span>
          {MODE_BUTTONS.map((button) => (
            <ToolButton key={button.mode} title={button.title} isActive={brushKind === null && !mountainTool.isSelected && !paintTool.isSelected && mode === button.mode} onClick={() => onChange(button.mode)}>
              <EditorIcon name={button.icon} size={14} />
            </ToolButton>
          ))}
        </div>
        {areBrushesAvailable && (
          <div className="scene-view__panel" role="group" aria-label="Кисти рельефа">
            <span className="scene-view__panel-caption">Рельеф</span>
            {BRUSH_BUTTONS.map((button) => (
              <ToolButton key={button.kind} title={button.title} isActive={brushKind === button.kind} onClick={() => onBrushChange(button.kind)}>
                <EditorIcon name={button.icon} size={14} />
              </ToolButton>
            ))}
            <ToolButton
              title={mountainTool.isEnabled ? "Поставить гору из штампа" : NO_STAMPS_TITLE}
              isActive={mountainTool.isSelected}
              isDisabled={!mountainTool.isEnabled}
              onClick={mountainTool.onSelect}
            >
              <EditorIcon name="terrain-mountain" size={14} />
            </ToolButton>
          </div>
        )}
        {areBrushesAvailable && (
          <div className="scene-view__panel" role="group" aria-label="Покраска">
            <span className="scene-view__panel-caption">Покраска</span>
            <ToolButton
              title={paintTool.isEnabled ? "Покрасить землю материалом (с Shift — стереть)" : NO_MATERIALS_TITLE}
              isActive={paintTool.isSelected}
              isDisabled={!paintTool.isEnabled}
              onClick={paintTool.onSelect}
            >
              <EditorIcon name="terrain-paint" size={14} />
            </ToolButton>
          </div>
        )}
      </div>
      {areBrushesAvailable && mountainTool.isSelected && (
        <div className="scene-view__tool-row">
          <MountainToolFields {...mountainTool.fields} />
        </div>
      )}
      {areBrushesAvailable && paintTool.isSelected && <PaintBrushFields {...paintTool.fields} />}
      {areBrushesAvailable && brushKind !== null && (
        <TerrainBrushFields
          size={brushSize}
          strength={brushStrength}
          onSizeChange={onBrushSizeChange}
          onStrengthChange={onBrushStrengthChange}
          water={water}
          onWaterChange={onWaterChange}
        />
      )}
    </div>
  );
}
