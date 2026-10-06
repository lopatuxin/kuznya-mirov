/** Поля, у которых нажатие не флажок и не кнопка, а ввод: текст, число, выбор из списка. */
const NON_TYPING_INPUTS = new Set(["checkbox", "radio", "color", "range", "button"]);

/** В элемент печатают — клавиши сцены ему не мешают: Esc отменяет ввод, W и E пишут буквы. */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  if (target instanceof HTMLInputElement) return !NON_TYPING_INPUTS.has(target.type);
  return target instanceof HTMLElement && target.isContentEditable;
}

/** Захват доносит отпускание до холста, даже если кнопку отпустили за ним; у указателя, которого уже нет, захвата нет — и жесту он не нужен. */
export function capturePointer(canvas: HTMLCanvasElement, pointerId: number): void {
  try {
    canvas.setPointerCapture(pointerId);
  } catch {
    // нет такого активного указателя
  }
}

/**
 * «Запуск» и продолжение после паузы отдают клавиатуру игре сразу, как Play в Unity — «Партия в редакторе», требование 33:
 * фокус на сцене без щелчка по ней. Поле, что было в фокусе, при этом теряет его и записывается.
 */
export function focusSceneWhenGameStarts(canvas: Pick<HTMLCanvasElement, "focus"> | null, isGameInputActive: boolean): void {
  if (isGameInputActive) canvas?.focus({ preventScroll: true });
}
