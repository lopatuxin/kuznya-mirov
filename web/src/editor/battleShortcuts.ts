/** Минимум `KeyboardEvent`, которого хватает сочетаниям партии — «Редактор», требование 1. */
export type ShortcutKeyEvent = { code: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean };

function isPlainCtrl(event: ShortcutKeyEvent, code: string): boolean {
  return event.code === code && event.ctrlKey && !event.shiftKey && !event.altKey;
}

/** Ctrl+P — «Запуск», а во время партии и повтора «Стоп»; окно печати браузера не открывается. */
export function isPlayStopShortcut(event: ShortcutKeyEvent): boolean {
  return isPlainCtrl(event, "KeyP");
}

/** Ctrl+Shift+P — «Пауза» и продолжение. */
export function isPauseResumeShortcut(event: ShortcutKeyEvent): boolean {
  return event.code === "KeyP" && event.ctrlKey && event.shiftKey && !event.altKey;
}

/** Ctrl+Alt+P — «Шаг». */
export function isStepShortcut(event: ShortcutKeyEvent): boolean {
  return event.code === "KeyP" && event.ctrlKey && event.altKey && !event.shiftKey;
}

/** Любое из трёх сочетаний партии — все три перехватываются раньше браузера, при любом фокусе. */
export function isBattleTransportShortcut(event: ShortcutKeyEvent): boolean {
  return isPlayStopShortcut(event) || isPauseResumeShortcut(event) || isStepShortcut(event);
}

/** ← и → — шаг назад и вперёд по шкале повтора, требование 29 (не в поле ввода — проверяет вызывающая сторона). */
export function isReplaySeekShortcut(event: ShortcutKeyEvent): "back" | "forward" | null {
  if (event.ctrlKey || event.shiftKey || event.altKey) return null;
  if (event.code === "ArrowLeft") return "back";
  if (event.code === "ArrowRight") return "forward";
  return null;
}
