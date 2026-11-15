// «Интерфейс игры» → «Переключатель»: полный экран страница разворачивает или сворачивает по просьбе движка
// и сообщает ему, каково окно на деле, — по событию, а не по тому, что просили.

export type FullscreenTarget = { requestFullscreen(): Promise<void> };

export type FullscreenDocument = {
  readonly fullscreenElement: Element | null;
  exitFullscreen(): Promise<void>;
  addEventListener(type: "fullscreenchange", listener: () => void): void;
};

export type FullscreenReceiver = { set_fullscreen(on: boolean): void };

/** Браузер разворачивает только по нажатию игрока и может отказать — отказ не ошибка игры, окно остаётся прежним. */
export async function toggleFullscreen(target: FullscreenTarget, doc: FullscreenDocument): Promise<void> {
  try {
    if (doc.fullscreenElement) await doc.exitFullscreen();
    else await target.requestFullscreen();
  } catch (error) {
    console.warn(`страница: окно не удалось развернуть или свернуть (${error instanceof Error ? error.message : String(error)})`);
  }
}

export function reportFullscreenToEngine(engine: FullscreenReceiver, doc: FullscreenDocument): void {
  doc.addEventListener("fullscreenchange", () => engine.set_fullscreen(doc.fullscreenElement !== null));
}
