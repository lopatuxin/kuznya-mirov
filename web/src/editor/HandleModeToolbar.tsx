import { EditorIcon, type EditorIconName } from "./EditorIcon";
import type { HandleMode } from "./handleGeometry";
import { ImprintToolFields, type ImprintToolFieldsProps } from "./ImprintToolFields";
import { BrushSizeFields } from "./BrushSizeFields";
import type { BrushKind } from "./terrainBrush";
import type { TerrainWater } from "./terrainFile";
import { keepSceneFocus, ToolMenu, ToolMenuItem } from "./ToolMenu";
import { WaterFields } from "./WaterFields";

/** Инструмент «Отпечаток» и его поля («Правка сцены», требования 22–23). */
type ImprintToolbar = {
  isSelected: boolean;
  /** Инструмент выбирается, когда в игре объявлены штампы. */
  isEnabled: boolean;
  fields: ImprintToolFieldsProps;
  onSelect: () => void;
};

/** Группа «Материалы»: выбранный материал — это кисть, которая красит им землю («Покраска», требования 1–4). */
type MaterialsToolbar = {
  /** Кисть красит: выбран материал. */
  isSelected: boolean;
  /** Группа доступна, когда в игре объявлены материалы. */
  isEnabled: boolean;
  /** Материалы `files.materials` в порядке объявления. */
  materialNames: readonly string[];
  /** Последний выбранный материал — им красит щелчок по самой кнопке группы. */
  material: string;
  /** Материалы, которыми нельзя красить: слоёв уже восемь, а их среди слоёв нет. */
  blockedMaterials: ReadonlySet<string>;
  /** Выбор материала в окошке — кисть сразу красит им. */
  onMaterialSelect: (material: string) => void;
  onSelect: () => void;
};

/** Инструмент группы «Рельеф»: кисть или «Отпечаток». */
export type TerrainTool = BrushKind | "imprint";

const NO_STAMPS_TITLE = "Нет штампов: объяви files.stamps в game.json";
const NO_MATERIALS_TITLE = "Нет материалов: объяви files.materials в game.json";
const BLOCKED_HINT = "слоёв уже восемь";

type HandleModeToolbarProps = {
  mode: HandleMode;
  /** Плоская сцена: только перенос и масштаб, без поворота («Правка сцены», требование 15). */
  isThreeDimensionalScene: boolean;
  /** Выбранная кисть; `null` — выбраны ручки. Выбран всегда один инструмент. */
  brushKind: BrushKind | null;
  /** Кисти есть только в трёхмерной сцене вне партии («Кисти рельефа», требование 4). */
  areBrushesAvailable: boolean;
  /** Последний выбранный инструмент группы «Рельеф» — его включает щелчок по самой кнопке группы. */
  lastTerrainTool: TerrainTool;
  brushSize: number;
  brushStrength: number;
  water: TerrainWater | null;
  imprintTool: ImprintToolbar;
  materialsTool: MaterialsToolbar;
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

const TERRAIN_TOOLS: { tool: TerrainTool; icon: EditorIconName; label: string; hint?: string }[] = [
  { tool: "raise", icon: "terrain-raise", label: "Поднять", hint: "Shift — опустить" },
  { tool: "level", icon: "terrain-level", label: "Выровнять", hint: "до точки нажатия" },
  { tool: "smooth", icon: "terrain-smooth", label: "Сгладить" },
  { tool: "imprint", icon: "terrain-imprint", label: "Отпечаток", hint: "штамп горы или равнины" },
];

type ToolButtonProps = { title: string; isActive: boolean; onClick: () => void; children: React.ReactNode };

function ToolButton({ title, isActive, onClick, children }: ToolButtonProps): React.JSX.Element {
  return (
    <button
      type="button"
      className={`scene-tools__button${isActive ? " scene-tools__button--active" : ""}`}
      title={title}
      aria-label={title}
      aria-pressed={isActive}
      onMouseDown={keepSceneFocus}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

/** Инструмент группы «Рельеф», который включает щелчок по её кнопке: последний выбранный, а недоступный «Отпечаток» — «Поднять». */
export function terrainGroupTool(lastTerrainTool: TerrainTool, isImprintEnabled: boolean): TerrainTool {
  return lastTerrainTool === "imprint" && !isImprintEnabled ? "raise" : lastTerrainTool;
}

type TerrainMenuContentProps = Pick<HandleModeToolbarProps, "brushKind" | "brushSize" | "brushStrength" | "imprintTool" | "onBrushSizeChange" | "onBrushStrengthChange"> & {
  groupTool: TerrainTool;
  onSelect: (tool: TerrainTool) => void;
  /** Инструмент или штамп выбран — окошко закрывается, как меню: следующий Esc снимает инструмент, а не окошко. */
  onPicked: () => void;
};

/** Окошко группы «Рельеф»: кисти и «Отпечаток», под ними настройки инструмента группы — размер и сила кисти или штамп. */
export function TerrainMenuContent({ brushKind, brushSize, brushStrength, imprintTool, onBrushSizeChange, onBrushStrengthChange, groupTool, onSelect, onPicked }: TerrainMenuContentProps): React.JSX.Element {
  return (
    <>
      <div className="tool-menu__items">
        {TERRAIN_TOOLS.map(({ tool, icon, label, hint }) => {
          const isImprint = tool === "imprint";
          return (
            <ToolMenuItem
              key={tool}
              icon={icon}
              label={label}
              hint={hint}
              title={isImprint && !imprintTool.isEnabled ? NO_STAMPS_TITLE : undefined}
              isActive={isImprint ? imprintTool.isSelected : brushKind === tool}
              isDisabled={isImprint && !imprintTool.isEnabled}
              onSelect={() => {
                onSelect(tool);
                onPicked();
              }}
            />
          );
        })}
      </div>
      <div className="tool-menu__separator" />
      {groupTool === "imprint" ? (
        <ImprintToolFields
          {...imprintTool.fields}
          onStampChange={(stamp) => {
            imprintTool.fields.onStampChange(stamp);
            onPicked();
          }}
        />
      ) : (
        <BrushSizeFields size={brushSize} strength={brushStrength} onSizeChange={onBrushSizeChange} onStrengthChange={onBrushStrengthChange} />
      )}
    </>
  );
}

type MaterialsMenuContentProps = Pick<HandleModeToolbarProps, "materialsTool" | "brushSize" | "brushStrength" | "onBrushSizeChange" | "onBrushStrengthChange"> & {
  /** Материал выбран — окошко закрывается, как меню. */
  onPicked: () => void;
};

/** Окошко группы «Материалы»: материалы игры по порядку объявления — выбранным красит кисть, — под ними размер и сила кисти. */
export function MaterialsMenuContent({ materialsTool, brushSize, brushStrength, onBrushSizeChange, onBrushStrengthChange, onPicked }: MaterialsMenuContentProps): React.JSX.Element {
  return (
    <>
      <div className="tool-menu__items">
        {materialsTool.materialNames.map((name) => {
          const isBlocked = materialsTool.blockedMaterials.has(name);
          return (
            <ToolMenuItem
              key={name}
              label={name}
              hint={isBlocked ? BLOCKED_HINT : undefined}
              isActive={materialsTool.isSelected && name === materialsTool.material}
              isDisabled={isBlocked}
              onSelect={() => {
                materialsTool.onMaterialSelect(name);
                onPicked();
              }}
            />
          );
        })}
      </div>
      <div className="tool-menu__separator" />
      <BrushSizeFields size={brushSize} strength={brushStrength} onSizeChange={onBrushSizeChange} onStrengthChange={onBrushStrengthChange} />
    </>
  );
}

/**
 * Инструменты сцены в верхней полосе — «Редактор», требование 10, и «Кисти рельефа», требования 1–3: виды
 * ручек значками (те же режимы, что клавиши `W`, `E`, `R`; в плоской сцене только `W` и `R`), затем в трёхмерной группы «Рельеф», «Материалы» и «Вода» кнопками с
 * выпадающим окошком — инструменты группы и их настройки в полосе постоянно не стоят.
 */
export function HandleModeToolbar({
  mode,
  isThreeDimensionalScene,
  brushKind,
  areBrushesAvailable,
  lastTerrainTool,
  brushSize,
  brushStrength,
  water,
  imprintTool,
  materialsTool,
  onChange,
  onBrushChange,
  onBrushSizeChange,
  onBrushStrengthChange,
  onWaterChange,
}: HandleModeToolbarProps): React.JSX.Element {
  const groupTool = terrainGroupTool(lastTerrainTool, imprintTool.isEnabled);
  const groupToolInfo = TERRAIN_TOOLS.find(({ tool }) => tool === groupTool) ?? TERRAIN_TOOLS[0];
  const isTerrainSelected = brushKind !== null || imprintTool.isSelected;
  const selectTerrainTool = (tool: TerrainTool): void => (tool === "imprint" ? imprintTool.onSelect() : onBrushChange(tool));

  return (
    <>
      <div className="scene-tools__group" role="group" aria-label="Ручки объекта">
        {MODE_BUTTONS.filter((button) => isThreeDimensionalScene || button.mode !== "rotate").map((button) => (
          <ToolButton key={button.mode} title={button.title} isActive={!isTerrainSelected && !materialsTool.isSelected && mode === button.mode} onClick={() => onChange(button.mode)}>
            <EditorIcon name={button.icon} size={15} />
          </ToolButton>
        ))}
      </div>
      {areBrushesAvailable && (
        <>
          <ToolMenu
            label="Рельеф"
            caption="Рельеф"
            icon={groupToolInfo.icon}
            title={`Рельеф: ${groupToolInfo.label}`}
            isActive={isTerrainSelected}
            onActivate={() => selectTerrainTool(groupTool)}
          >
            {(close) => (
              <TerrainMenuContent
                brushKind={brushKind}
                brushSize={brushSize}
                brushStrength={brushStrength}
                imprintTool={imprintTool}
                onBrushSizeChange={onBrushSizeChange}
                onBrushStrengthChange={onBrushStrengthChange}
                groupTool={groupTool}
                onSelect={selectTerrainTool}
                onPicked={close}
              />
            )}
          </ToolMenu>
          <ToolMenu
            label="Материалы"
            caption="Материалы"
            icon="materials"
            title={materialsTool.isEnabled ? `Красить материалом ${materialsTool.material} (с Shift — стереть)` : NO_MATERIALS_TITLE}
            isActive={materialsTool.isSelected}
            isDisabled={!materialsTool.isEnabled}
            onActivate={materialsTool.onSelect}
          >
            {(close) => (
              <MaterialsMenuContent
                materialsTool={materialsTool}
                brushSize={brushSize}
                brushStrength={brushStrength}
                onBrushSizeChange={onBrushSizeChange}
                onBrushStrengthChange={onBrushStrengthChange}
                onPicked={close}
              />
            )}
          </ToolMenu>
          <ToolMenu label="Вода" caption="Вода" icon="water" title="Вода рельефа: есть ли она, уровень и цвет">
            <WaterFields water={water} onWaterChange={onWaterChange} />
          </ToolMenu>
        </>
      )}
    </>
  );
}
