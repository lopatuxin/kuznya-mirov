import { BrushNumberField, clampedTo } from "./TerrainBrushFields";
import { BRUSH_SIZE_LIMITS, BRUSH_STRENGTH_LIMITS } from "./terrainBrush";

export type PaintBrushFieldsProps = {
  size: number;
  strength: number;
  /** Материалы `files.materials` в порядке объявления. */
  materialNames: readonly string[];
  material: string;
  /** Материалы, которыми нельзя красить: слоёв уже восемь, а их среди слоёв нет. */
  blockedMaterials: ReadonlySet<string>;
  onSizeChange: (size: number) => void;
  onStrengthChange: (strength: number) => void;
  onMaterialChange: (material: string) => void;
};

const BLOCKED_SUFFIX = " — слоёв уже восемь";

/** Поля кисти «Покрасить» в полосе над сценой — «Покраска», требования 2–3: общие «Размер» и «Сила» и «Материал»; воды нет. */
export function PaintBrushFields({ size, strength, materialNames, material, blockedMaterials, onSizeChange, onStrengthChange, onMaterialChange }: PaintBrushFieldsProps): React.JSX.Element {
  return (
    <div className="scene-view__tool-row">
      <div className="scene-view__panel scene-view__panel--fields" role="group" aria-label="Кисть">
        <span className="scene-view__panel-caption">Кисть</span>
        <BrushNumberField label="Размер" value={size} normalize={clampedTo(BRUSH_SIZE_LIMITS)} onCommit={onSizeChange} />
        <BrushNumberField label="Сила" value={strength} normalize={clampedTo(BRUSH_STRENGTH_LIMITS)} onCommit={onStrengthChange} />
        <label className="scene-view__field">
          Материал
          <select className="scene-view__field-input scene-view__field-input--select" value={material} onChange={(event) => onMaterialChange(event.target.value)}>
            {materialNames.map((name) => (
              <option key={name} value={name} disabled={blockedMaterials.has(name)}>
                {blockedMaterials.has(name) ? `${name}${BLOCKED_SUFFIX}` : name}
              </option>
            ))}
          </select>
        </label>
      </div>
    </div>
  );
}
