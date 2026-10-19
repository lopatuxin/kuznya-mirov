import type { Engine } from "engine";

/** Щелчок колеса в точках прокрутки — «Мир на экране», «Камера игрока», требование 5: `mouse_wheel` считает щелчки от этого числа. */
const POINTS_PER_WHEEL_CLICK = 100;
const WHEEL_LINES_PER_CLICK = 3;
const DOM_DELTA_LINE = 1;
const DOM_DELTA_PAGE = 2;

const MIDDLE_BUTTON = 1;
const RIGHT_BUTTON = 2;

/** Вызовы движка, которыми страница игры отдаёт камере игрока колесо и кнопки. */
export type CameraMouseEngine = Pick<Engine, "mouse_move" | "mouse_wheel" | "pan_down" | "pan_up">;

/** Правая и средняя кнопки тащат карту (`pan_down`/`pan_up`); левая идёт в мир как `MouseLeft`. */
export function isPanButton(button: number): boolean {
  return button === MIDDLE_BUTTON || button === RIGHT_BUTTON;
}

/** `deltaY` события `wheel` в точках прокрутки: строки и страницы Firefox сводятся к тем же 100 точкам на щелчок, что у пикселей Chrome. */
export function wheelDeltaPoints(deltaY: number, deltaMode: number): number {
  if (deltaMode === DOM_DELTA_LINE) return (deltaY * POINTS_PER_WHEEL_CLICK) / WHEEL_LINES_PER_CLICK;
  if (deltaMode === DOM_DELTA_PAGE) return deltaY * POINTS_PER_WHEEL_CLICK;
  return deltaY;
}

/**
 * Камера игрока на странице игры — «Камера игрока», требования 9 и 21: колесо над холстом приближает её и не
 * прокручивает страницу, правая и средняя кнопки, нажатые над холстом, тащат карту, а меню правой кнопки над
 * холстом не открывается. Точку под указателем движок знает только из `mouse_move`, поэтому колесо и нажатие
 * шлют её перед собой. Отпускание слушает окно целиком: кнопку могли отпустить за холстом.
 */
export function attachCameraMouse(engine: CameraMouseEngine, canvas: HTMLCanvasElement): void {
  canvas.addEventListener("contextmenu", (event) => event.preventDefault());
  canvas.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      engine.mouse_move(event.clientX, event.clientY);
      engine.mouse_wheel(wheelDeltaPoints(event.deltaY, event.deltaMode));
    },
    { passive: false },
  );
  canvas.addEventListener("mousedown", (event) => {
    if (!isPanButton(event.button)) return;
    // Средняя кнопка без этого включает автопрокрутку Windows.
    event.preventDefault();
    engine.mouse_move(event.clientX, event.clientY);
    engine.pan_down();
  });
  window.addEventListener("mouseup", (event) => {
    if (isPanButton(event.button)) engine.pan_up();
  });
}
