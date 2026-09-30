import type { Vec2 } from "./objectPlacement";

/** «Кисти рельефа», требование 7: окружность кисти — 64 точки. */
const RING_POINT_COUNT = 64;
const RING_COLOR = "#f4f7fb";
const RING_HALO = "rgba(6, 8, 14, 0.75)";
const RING_WIDTH_PX = 2;
const MARK_HALF_SIZE_PX = 5;

/** Точки окружности радиуса `radius` вокруг `center` в местах сцены — высоту каждой берёт рельеф. */
export function brushRingPoints(center: Vec2, radius: number): Vec2[] {
  return Array.from({ length: RING_POINT_COUNT }, (_, index): Vec2 => {
    const angle = (2 * Math.PI * index) / RING_POINT_COUNT;
    return [center[0] + radius * Math.cos(angle), center[1] + radius * Math.sin(angle)];
  });
}

function strokeTwice(context: CanvasRenderingContext2D, trace: () => void): void {
  for (const [color, width] of [[RING_HALO, RING_WIDTH_PX + 2], [RING_COLOR, RING_WIDTH_PX]] as const) {
    context.beginPath();
    trace();
    context.strokeStyle = color;
    context.lineWidth = width;
    context.stroke();
  }
}

/**
 * Круг кисти поверх холста — «Кисти рельефа», требование 7: замкнутая линия по точкам окружности на
 * рельефе и метка в середине, светлая линия с тёмной обводкой, чтобы видно было на траве и на воде.
 * Точка за камерой (`undefined`) разрывает линию.
 */
export function drawBrushCircle(context: CanvasRenderingContext2D, ring: readonly (Vec2 | undefined)[], center: Vec2 | undefined, pixelRatio: number): void {
  context.save();
  context.scale(pixelRatio, pixelRatio);
  context.lineJoin = "round";
  context.lineCap = "round";
  strokeTwice(context, () => {
    let isPenDown = false;
    // Первая точка повторяется в конце — линия замыкается.
    for (const point of [...ring, ring[0]]) {
      if (point === undefined) {
        isPenDown = false;
        continue;
      }
      if (isPenDown) context.lineTo(point[0], point[1]);
      else context.moveTo(point[0], point[1]);
      isPenDown = true;
    }
  });
  if (center !== undefined) {
    strokeTwice(context, () => {
      context.moveTo(center[0] - MARK_HALF_SIZE_PX, center[1]);
      context.lineTo(center[0] + MARK_HALF_SIZE_PX, center[1]);
      context.moveTo(center[0], center[1] - MARK_HALF_SIZE_PX);
      context.lineTo(center[0], center[1] + MARK_HALF_SIZE_PX);
    });
  }
  context.restore();
}
