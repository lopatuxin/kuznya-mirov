import { CENTER_HALF_SIZE_PX, type HandleGeometry, type HandleHit, type HandleMode } from "./handleGeometry";
import type { Vec2 } from "./objectPlacement";

/** «Редактор», требование 9: цвета, как в Godot, — `x` красная, `y` синяя, высота зелёная. */
const AXIS_COLORS = { "axis-x": "#e5484d", "axis-y": "#3e8bff", "axis-z": "#46c26d" } as const;
type AxisHit = keyof typeof AXIS_COLORS;
const AXIS_HOVER_COLORS = { "axis-x": "#ff8a8d", "axis-y": "#8ab8ff", "axis-z": "#8fe6ab" } as const;
const CENTER_COLOR = "#d8dde6";
const CENTER_HOVER_COLOR = "#ffffff";
// Кольцо — поворот вокруг вертикали, поэтому цвета высоты, как в Godot.
const RING_COLOR = AXIS_COLORS["axis-z"];
const RING_HOVER_COLOR = AXIS_HOVER_COLORS["axis-z"];
const HANDLE_HALO = "rgba(6, 8, 14, 0.7)";
const ARROW_HEAD_PX = 11;
const TIP_BOX_HALF_PX = 5;

function strokeLine(context: CanvasRenderingContext2D, from: Vec2, to: Vec2, color: string, width: number): void {
  context.beginPath();
  context.moveTo(from[0], from[1]);
  context.lineTo(to[0], to[1]);
  context.strokeStyle = HANDLE_HALO;
  context.lineWidth = width + 2;
  context.stroke();
  context.strokeStyle = color;
  context.lineWidth = width;
  context.stroke();
}

function drawArrowHead(context: CanvasRenderingContext2D, from: Vec2, tip: Vec2, color: string): void {
  const angle = Math.atan2(tip[1] - from[1], tip[0] - from[0]);
  const spread = Math.PI / 7;
  context.beginPath();
  context.moveTo(tip[0], tip[1]);
  context.lineTo(tip[0] - ARROW_HEAD_PX * Math.cos(angle - spread), tip[1] - ARROW_HEAD_PX * Math.sin(angle - spread));
  context.lineTo(tip[0] - ARROW_HEAD_PX * Math.cos(angle + spread), tip[1] - ARROW_HEAD_PX * Math.sin(angle + spread));
  context.closePath();
  context.fillStyle = color;
  context.fill();
}

function drawTipBox(context: CanvasRenderingContext2D, tip: Vec2, color: string): void {
  context.fillStyle = color;
  context.strokeStyle = HANDLE_HALO;
  context.lineWidth = 1;
  context.fillRect(tip[0] - TIP_BOX_HALF_PX, tip[1] - TIP_BOX_HALF_PX, TIP_BOX_HALF_PX * 2, TIP_BOX_HALF_PX * 2);
  context.strokeRect(tip[0] - TIP_BOX_HALF_PX, tip[1] - TIP_BOX_HALF_PX, TIP_BOX_HALF_PX * 2, TIP_BOX_HALF_PX * 2);
}

function drawCenter(context: CanvasRenderingContext2D, center: Vec2, isHovered: boolean): void {
  const half = CENTER_HALF_SIZE_PX;
  context.fillStyle = isHovered ? CENTER_HOVER_COLOR : CENTER_COLOR;
  context.strokeStyle = HANDLE_HALO;
  context.lineWidth = 1.5;
  context.fillRect(center[0] - half, center[1] - half, half * 2, half * 2);
  context.strokeRect(center[0] - half, center[1] - half, half * 2, half * 2);
}

/** Ручки выбранного объекта поверх холста: стрелки с наконечниками при переносе, кольцо при повороте, стрелки с квадратами при масштабе. */
export function drawHandles(context: CanvasRenderingContext2D, geometry: HandleGeometry, mode: HandleMode, hovered: HandleHit | null, pixelRatio: number): void {
  context.save();
  context.scale(pixelRatio, pixelRatio);
  context.lineCap = "round";
  context.lineJoin = "round";

  if (mode === "rotate") {
    const color = hovered === "ring" ? RING_HOVER_COLOR : RING_COLOR;
    context.beginPath();
    geometry.ring.forEach((point, index) => (index === 0 ? context.moveTo(point[0], point[1]) : context.lineTo(point[0], point[1])));
    context.closePath();
    context.strokeStyle = HANDLE_HALO;
    context.lineWidth = 5;
    context.stroke();
    context.strokeStyle = color;
    context.lineWidth = 3;
    context.stroke();
    context.restore();
    return;
  }

  const arrows: [AxisHit, Vec2 | null][] = [
    ["axis-x", geometry.tipX],
    ["axis-y", geometry.tipY],
    ["axis-z", geometry.tipZ],
  ];
  for (const [hit, tip] of arrows) {
    if (tip === null) continue;
    const color = hovered === hit ? AXIS_HOVER_COLORS[hit] : AXIS_COLORS[hit];
    strokeLine(context, geometry.center, tip, color, 3);
    if (mode === "translate") drawArrowHead(context, geometry.center, tip, color);
    else drawTipBox(context, tip, color);
  }
  drawCenter(context, geometry.center, hovered === "center");
  context.restore();
}
