import type { Engine } from "engine";
import type { Vec2 } from "./objectPlacement";
import type { CanvasRect } from "./selectionDrawing";

/** Вид невидимого прямоугольника — как его называет вызов движка `outlines()`. */
export type OutlineKind = "solid" | "one_way" | "other";

export type SceneOutline = { id: number; kind: OutlineKind; rect: CanvasRect };

export type SceneOutlinesEngine = Pick<Engine, "outlines" | "object_rect">;

/** Щелчок не дальше стольких точек экрана от линии прямоугольника выбирает его — «Редактор», Фаза 37, требование 24. */
export const OUTLINE_HIT_TOLERANCE_PX = 6;

const HALO_COLOR = "rgba(6, 8, 14, 0.7)";
const HALO_EXTRA_WIDTH = 2;
const SOLID_FILL = "rgba(255, 200, 87, 0.18)";

type LineStyle = { color: string; width: number; dash: readonly number[] };

const SOLID_LINE: LineStyle = { color: "#ffc857", width: 2, dash: [] };
const ONE_WAY_TOP_LINE: LineStyle = { color: "#6fe0a8", width: 2, dash: [] };
const ONE_WAY_SIDE_LINE: LineStyle = { color: "#6fe0a8", width: 1.5, dash: [6, 4] };
const OTHER_LINE: LineStyle = { color: "#e6ebf2", width: 1, dash: [3, 3] };

/**
 * Невидимые прямоугольники плоской сцены на нынешнем мире — без кэша: список спрашивается у движка заново при каждом
 * кадре и щелчке, он уже учитывает перезагрузку файлов и правки мира («Редактор», Фаза 37, требование 23).
 * Объект, чей прямоугольник на холсте движок не назвал, пропускается.
 */
export function readSceneOutlines(engine: SceneOutlinesEngine): SceneOutline[] {
  const outlines: SceneOutline[] = [];
  for (const entry of engine.outlines() as { id: number; kind: OutlineKind }[]) {
    const rect = engine.object_rect(entry.id) as CanvasRect | undefined;
    if (rect !== undefined) outlines.push({ id: entry.id, kind: entry.kind, rect });
  }
  return outlines;
}

/** Линия с тёмной каймой под ней — видна и на светлом, и на тёмном фоне; толщина в точках экрана, от приближения не зависит. */
function strokeLine(context: CanvasRenderingContext2D, trace: () => void, line: LineStyle): void {
  context.setLineDash([...line.dash]);
  context.lineWidth = line.width + HALO_EXTRA_WIDTH;
  context.strokeStyle = HALO_COLOR;
  context.beginPath();
  trace();
  context.stroke();
  context.lineWidth = line.width;
  context.strokeStyle = line.color;
  context.beginPath();
  trace();
  context.stroke();
}

function drawOutline(context: CanvasRenderingContext2D, { kind, rect }: SceneOutline): void {
  const { x, y, width, height } = rect;
  if (kind === "solid") {
    context.fillStyle = SOLID_FILL;
    context.fillRect(x, y, width, height);
    strokeLine(context, () => context.rect(x, y, width, height), SOLID_LINE);
    return;
  }
  if (kind === "one_way") {
    strokeLine(context, () => {
      context.moveTo(x, y + height);
      context.lineTo(x, y);
      context.moveTo(x + width, y);
      context.lineTo(x + width, y + height);
      context.lineTo(x, y + height);
    }, ONE_WAY_SIDE_LINE);
    strokeLine(context, () => {
      context.moveTo(x, y);
      context.lineTo(x + width, y);
    }, ONE_WAY_TOP_LINE);
    return;
  }
  strokeLine(context, () => context.rect(x, y, width, height), OTHER_LINE);
}

/**
 * Рисует невидимые прямоугольники поверх холста — «Редактор», Фаза 37, требование 22: твёрдое — полупрозрачной
 * заливкой и сплошной линией, площадка — сплошной линией сверху и пунктиром по остальным сторонам, прочее — тонким пунктиром.
 */
export function drawSceneOutlines(context: CanvasRenderingContext2D, outlines: readonly SceneOutline[], pixelRatio: number): void {
  if (outlines.length === 0) return;
  context.save();
  context.scale(pixelRatio, pixelRatio);
  context.lineJoin = "miter";
  for (const outline of outlines) drawOutline(context, outline);
  context.restore();
}

/** Расстояние от точки до линии прямоугольника: снаружи — до ближайшей стороны или угла, внутри — до ближайшей стороны. */
function distanceToRectLine(rect: CanvasRect, point: Vec2): number {
  const outsideX = Math.max(rect.x - point[0], 0, point[0] - (rect.x + rect.width));
  const outsideY = Math.max(rect.y - point[1], 0, point[1] - (rect.y + rect.height));
  if (outsideX > 0 || outsideY > 0) return Math.hypot(outsideX, outsideY);
  return Math.min(point[0] - rect.x, rect.x + rect.width - point[0], point[1] - rect.y, rect.y + rect.height - point[1]);
}

/** Прямоугольник, к линии которого щелчок ближе всего, но не дальше `OUTLINE_HIT_TOLERANCE_PX`; из равных — нарисованный позже. */
export function outlineIdAtPoint(outlines: readonly SceneOutline[], point: Vec2): number | undefined {
  let bestId: number | undefined;
  let bestDistance = OUTLINE_HIT_TOLERANCE_PX;
  for (const outline of outlines) {
    const distance = distanceToRectLine(outline.rect, point);
    if (distance <= bestDistance) {
      bestId = outline.id;
      bestDistance = distance;
    }
  }
  return bestId;
}
