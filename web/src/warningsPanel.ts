import { formatErrors, type EngineError } from "./engineErrors";

export type WarningsBadge = { label: string; details: string };

/**
 * Русское согласование числительного с «предупреждение»: обычные правила окончаний
 * (1 — «предупреждение», 2–4 — «предупреждения», 5–20 и 0 — «предупреждений»), с исключением
 * для 11–14, которые попадают в «предупреждений», а не в «предупреждение»/«предупреждения» по
 * последней цифре.
 */
function pluralizeWarningWord(count: number): string {
  const mod10 = count % 10;
  const mod100 = count % 100;
  if (mod10 === 1 && mod100 !== 11) return "предупреждение";
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 10 || mod100 >= 20)) return "предупреждения";
  return "предупреждений";
}

/**
 * Данные для плашки предупреждений поверх идущей игры: свёрнутый счётчик (`label`) и
 * разворачиваемый по клику список в формате ошибок движка — файл, место, текст (`details`), тот
 * же, что и на экране ошибки, его читает и человек, и агент, правящий данные игры снаружи.
 * `null`, когда предупреждений нет: игра тогда идёт без какой-либо плашки над холстом.
 */
export function buildWarningsBadge(warnings: EngineError[]): WarningsBadge | null {
  if (warnings.length === 0) return null;
  return {
    label: `⚠ ${warnings.length} ${pluralizeWarningWord(warnings.length)}`,
    details: formatErrors(warnings),
  };
}

/**
 * `event.detail` различает источник активации кнопки: у клика мышью это номер клика в
 * последовательности (1, 2, 3 — двойной, тройной клик), всегда больше нуля; у активации с
 * клавиатуры (Enter/Space на сфокусированной кнопке) — всегда 0. Снимать фокус нужно только
 * после клика мышью: после клавиатурной активации кнопка должна остаться в фокусе, иначе
 * следующий Tab начинает обход заново, а свернуть список тем же Enter уже нельзя.
 */
export function shouldBlurAfterToggleActivation(event: Pick<MouseEvent, "detail">): boolean {
  return event.detail > 0;
}
