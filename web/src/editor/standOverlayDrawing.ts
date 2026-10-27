import type { StandBone, StandPose } from "./useAnimationStand";

type CanvasPoint = readonly [number, number];
type MaybePoint = CanvasPoint | null;

/** Кольца поворота выбранной кости, как их отдаёт `motion_rings`: середина кости и три кольца вокруг осей x, y, z ломаными. */
export type StandRings = { center: MaybePoint; rings: MaybePoint[][] };

export type StandOverlayFrame = {
  pose: StandPose;
  bones: readonly StandBone[];
  selectedBone: number | null;
  rings: StandRings | undefined;
  /** Кольцо под указателем или в руке: 0, 1 или 2. */
  activeRing: number | null;
  /** Сдвиг холста движка внутри холста поверх него, в точках окна. */
  offset: CanvasPoint;
  pixelRatio: number;
};

const RING_COLORS = ["#ff6b6b", "#6bd68a", "#6bb2ff"] as const;
const JOINT_COLOR = "rgba(230, 233, 239, 0.4)";
const SELECTED_COLOR = "#5ab0ff";

function shifted(point: CanvasPoint, offset: CanvasPoint): CanvasPoint {
  return [point[0] + offset[0], point[1] + offset[1]];
}

function drawRing(context: CanvasRenderingContext2D, ring: readonly MaybePoint[], offset: CanvasPoint): void {
  context.beginPath();
  let isPenDown = false;
  for (const point of ring) {
    if (!point) {
      isPenDown = false;
      continue;
    }
    const [x, y] = shifted(point, offset);
    if (isPenDown) context.lineTo(x, y);
    else context.moveTo(x, y);
    isPenDown = true;
  }
  context.stroke();
}

/**
 * Что редактор рисует поверх просмотра движка — «Анимация», требования 10–11: суставы, кость, выбранная щелчком, и кольца
 * её поворота. Саму модель рисует движок.
 */
export function drawStandOverlay(context: CanvasRenderingContext2D, frame: StandOverlayFrame): void {
  const { pose, bones, selectedBone, rings, activeRing, offset, pixelRatio } = frame;
  context.save();
  context.scale(pixelRatio, pixelRatio);
  context.lineCap = "round";
  context.lineJoin = "round";

  context.fillStyle = JOINT_COLOR;
  for (const point of pose.points) {
    if (!point) continue;
    const [x, y] = shifted(point, offset);
    context.beginPath();
    context.arc(x, y, 2.5, 0, Math.PI * 2);
    context.fill();
  }

  const selected = selectedBone === null ? null : pose.points[selectedBone];
  if (selectedBone !== null && selected) {
    context.strokeStyle = SELECTED_COLOR;
    context.lineWidth = 3;
    context.beginPath();
    const [x, y] = shifted(selected, offset);
    bones.forEach((bone, index) => {
      const child = pose.points[index];
      if (bone.parent !== selectedBone || !child) return;
      const [childX, childY] = shifted(child, offset);
      context.moveTo(x, y);
      context.lineTo(childX, childY);
    });
    context.stroke();
    context.fillStyle = SELECTED_COLOR;
    context.beginPath();
    context.arc(x, y, 5, 0, Math.PI * 2);
    context.fill();
  }

  if (rings) {
    rings.rings.forEach((ring, axis) => {
      context.strokeStyle = RING_COLORS[axis] ?? SELECTED_COLOR;
      context.lineWidth = axis === activeRing ? 4 : 2;
      context.globalAlpha = axis === activeRing ? 1 : 0.85;
      drawRing(context, ring, offset);
    });
    context.globalAlpha = 1;
  }
  context.restore();
}
