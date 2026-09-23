import { useEffect, useState } from "react";

export const MIN_PANEL_WIDTH = 200;
export const MAX_PANEL_WIDTH = 560;
const SAVE_DELAY_MS = 300;

function clampPanelWidth(width: number): number {
  return Math.round(Math.min(MAX_PANEL_WIDTH, Math.max(MIN_PANEL_WIDTH, width)));
}

function readStoredWidth(storageKey: string): number | null {
  try {
    const stored = Number(localStorage.getItem(storageKey));
    return Number.isFinite(stored) && stored > 0 ? clampPanelWidth(stored) : null;
  } catch {
    return null;
  }
}

/**
 * Ширина боковой панели окна проекта, которую автор тянет мышью. Запоминается в браузере, когда
 * перетаскивание замерло, чтобы не выставлять её заново после обновления страницы; хранилище
 * недоступно — ширина по умолчанию.
 */
export function useStoredPanelWidth(storageKey: string, defaultWidth: number): [number, (width: number) => void] {
  const [width, setWidth] = useState(() => readStoredWidth(storageKey) ?? defaultWidth);

  useEffect(() => {
    const timer = setTimeout(() => {
      try {
        localStorage.setItem(storageKey, String(width));
      } catch {
        // Ширина просто не запомнится до следующего открытия.
      }
    }, SAVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [storageKey, width]);

  return [width, (nextWidth: number) => setWidth(clampPanelWidth(nextWidth))];
}
