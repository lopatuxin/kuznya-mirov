export type CanvasLayout = {
  cssWidth: number;
  cssHeight: number;
  bufferWidth: number;
  bufferHeight: number;
};

/**
 * Холст растянут на всё окно — сцену внутрь него вписывает уже движок (см. `Engine.resize`),
 * странице остаётся только сообщить размер окна. `cssWidth`/`cssHeight` идут в `canvas.style`, чтобы
 * разметка совпадала с этим расчётом; `bufferWidth`/`bufferHeight` — размер буфера холста в
 * физических пикселях с учётом `devicePixelRatio`, их передавать в `canvas.width`/`height` и в
 * `engine.resize`.
 */
export function computeCanvasLayout(
  viewportWidth: number,
  viewportHeight: number,
  devicePixelRatio: number,
): CanvasLayout {
  return {
    cssWidth: viewportWidth,
    cssHeight: viewportHeight,
    bufferWidth: Math.max(1, Math.round(viewportWidth * devicePixelRatio)),
    bufferHeight: Math.max(1, Math.round(viewportHeight * devicePixelRatio)),
  };
}
