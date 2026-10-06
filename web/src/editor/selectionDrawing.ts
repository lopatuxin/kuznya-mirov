import type { Vec2 } from "./objectPlacement";

export type CanvasRect = { x: number; y: number; width: number; height: number };

const SELECTION_COLOR = "#5ab0ff";
const SELECTION_FILL = "rgba(90, 176, 255, 0.1)";
const SELECTION_HALO = "rgba(6, 8, 14, 0.6)";
const LABEL_TEXT_COLOR = "#06121f";
const LABEL_FONT = '600 11px "EditorUi", "Segoe UI", sans-serif';
const LABEL_HEIGHT = 18;
const LABEL_GAP = 4;

function placeSelectionLabel(rect: CanvasRect, labelWidth: number, canvasWidth: number, canvasHeight: number): { x: number; y: number } {
  const x = Math.min(Math.max(0, rect.x), Math.max(0, canvasWidth - labelWidth));
  const above = rect.y - LABEL_HEIGHT - LABEL_GAP;
  if (above >= 0) return { x, y: above };
  const below = rect.y + rect.height + LABEL_GAP;
  if (below + LABEL_HEIGHT <= canvasHeight) return { x, y: below };
  return { x, y: Math.max(0, rect.y) + LABEL_GAP };
}

function drawSelectionLabel(context: CanvasRenderingContext2D, rect: CanvasRect, label: string, canvasWidth: number, canvasHeight: number): void {
  context.font = LABEL_FONT;
  const labelWidth = Math.ceil(context.measureText(label).width) + 12;
  const position = placeSelectionLabel(rect, labelWidth, canvasWidth, canvasHeight);
  context.fillStyle = SELECTION_COLOR;
  context.beginPath();
  context.roundRect(position.x, position.y, labelWidth, LABEL_HEIGHT, 4);
  context.fill();
  context.fillStyle = LABEL_TEXT_COLOR;
  context.textBaseline = "middle";
  context.fillText(label, position.x + 6, position.y + LABEL_HEIGHT / 2 + 0.5);
}

export function drawSelection(context: CanvasRenderingContext2D, rect: CanvasRect, label: string | null, pixelRatio: number): void {
  const canvasWidth = context.canvas.width / pixelRatio;
  const canvasHeight = context.canvas.height / pixelRatio;
  const frameX = rect.x + 1;
  const frameY = rect.y + 1;
  const frameWidth = Math.max(0, rect.width - 2);
  const frameHeight = Math.max(0, rect.height - 2);

  context.save();
  context.scale(pixelRatio, pixelRatio);
  context.fillStyle = SELECTION_FILL;
  context.fillRect(rect.x, rect.y, rect.width, rect.height);
  // Тёмная кайма под рамкой — чтобы рамку было видно и на голубом, и на светлом объекте.
  context.lineWidth = 4;
  context.strokeStyle = SELECTION_HALO;
  context.strokeRect(frameX, frameY, frameWidth, frameHeight);
  context.lineWidth = 2;
  context.strokeStyle = SELECTION_COLOR;
  context.strokeRect(frameX, frameY, frameWidth, frameHeight);

  if (label !== null) drawSelectionLabel(context, rect, label, canvasWidth, canvasHeight);
  context.restore();
}

/**
 * Рамка трёхмерной сцены — «Редактор», требование 8: четырёхугольник по четырём углам прямоугольника
 * объекта на земле, как их видит камера, цветом и видом рамки плоской сцены; подпись — над верхним углом.
 */
export function drawSelectionQuad(context: CanvasRenderingContext2D, corners: readonly Vec2[], label: string | null, pixelRatio: number): void {
  const canvasWidth = context.canvas.width / pixelRatio;
  const canvasHeight = context.canvas.height / pixelRatio;

  context.save();
  context.scale(pixelRatio, pixelRatio);
  context.beginPath();
  corners.forEach((corner, index) => (index === 0 ? context.moveTo(corner[0], corner[1]) : context.lineTo(corner[0], corner[1])));
  context.closePath();
  context.fillStyle = SELECTION_FILL;
  context.fill();
  context.lineJoin = "round";
  context.lineWidth = 4;
  context.strokeStyle = SELECTION_HALO;
  context.stroke();
  context.lineWidth = 2;
  context.strokeStyle = SELECTION_COLOR;
  context.stroke();

  if (label !== null) {
    const top = corners.reduce((highest, corner) => (corner[1] < highest[1] ? corner : highest));
    drawSelectionLabel(context, { x: top[0], y: top[1], width: 0, height: 0 }, label, canvasWidth, canvasHeight);
  }
  context.restore();
}

const BOUNDARY_COLOR = "#f4f7fb";
const BOUNDARY_HALO = "rgba(6, 8, 14, 0.7)";

/**
 * Граница плоской сцены поверх холста — «Редактор», «Сцена», требование 14: тонкая линия по прямоугольнику
 * сцены между его углами `from` и `to` (точки холста), светлая на тёмной кайме — видна и на светлом, и на тёмном фоне.
 */
export function drawSceneBoundary(context: CanvasRenderingContext2D, from: Vec2, to: Vec2, pixelRatio: number): void {
  context.save();
  context.scale(pixelRatio, pixelRatio);
  context.lineJoin = "miter";
  context.lineWidth = 3;
  context.strokeStyle = BOUNDARY_HALO;
  context.strokeRect(from[0], from[1], to[0] - from[0], to[1] - from[1]);
  context.lineWidth = 1;
  context.strokeStyle = BOUNDARY_COLOR;
  context.strokeRect(from[0], from[1], to[0] - from[0], to[1] - from[1]);
  context.restore();
}
