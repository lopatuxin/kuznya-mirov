export type CanvasLayout = {
  cssWidth: number;
  cssHeight: number;
  bufferWidth: number;
  bufferHeight: number;
};

/**
 * Вписывает холст в окно целиком, сохраняя соотношение сторон сцены (ширина:высота в клетках из
 * `game.json`) — без этого клетка сцены рисуется не квадратной. `cssWidth`/`cssHeight` идут в
 * `canvas.style`, чтобы разметка совпадала с этим расчётом; `bufferWidth`/`bufferHeight` — размер
 * буфера холста в физических пикселях с учётом `devicePixelRatio`, их передавать в
 * `canvas.width`/`height` и в `engine.resize`.
 */
export function computeCanvasLayout(
  sceneWidth: number,
  sceneHeight: number,
  viewportWidth: number,
  viewportHeight: number,
  devicePixelRatio: number,
): CanvasLayout {
  const sceneAspect = sceneWidth / sceneHeight;
  const viewportAspect = viewportWidth / viewportHeight;
  const isViewportWider = viewportAspect > sceneAspect;

  const cssWidth = isViewportWider ? viewportHeight * sceneAspect : viewportWidth;
  const cssHeight = isViewportWider ? viewportHeight : viewportWidth / sceneAspect;

  return {
    cssWidth,
    cssHeight,
    bufferWidth: Math.max(1, Math.round(cssWidth * devicePixelRatio)),
    bufferHeight: Math.max(1, Math.round(cssHeight * devicePixelRatio)),
  };
}
