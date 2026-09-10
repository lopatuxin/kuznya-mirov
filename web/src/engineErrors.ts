export type EngineError = {
  file: string;
  path: string;
  message: string;
  line: number | null;
  column: number | null;
};

/**
 * Одна строка ошибки или предупреждения предстартовой проверки движка: файл, место внутри него
 * (пусто, если места ещё нет — как у битого JSON), строка и столбец внутри файла (`null`, если
 * позицию определить не удалось — тогда она уже вшита в текст сообщения) и текст. Предупреждения
 * используют этот же тип и этот же формат, что и ошибки (см. «Формат игры» → «Проверка данных
 * перед запуском»).
 */
export function formatError(error: EngineError): string {
  const position = error.line !== null && error.column !== null ? ` (строка ${error.line}, столбец ${error.column})` : "";
  return error.path
    ? `${error.file}: ${error.path}${position} — ${error.message}`
    : `${error.file}${position} — ${error.message}`;
}

/**
 * Сворачивает список ошибок или предупреждений предстартовой проверки движка в один текст на
 * русском, по строке на запись, для показа вместо игры или в плашке предупреждений.
 */
export function formatErrors(errors: EngineError[]): string {
  return errors.map(formatError).join("\n");
}

/**
 * Текст экрана ошибки: сами ошибки, а если проверка успела собрать ещё и предупреждения до того,
 * как отказала, — отдельным разделом ниже, чтобы было видно, что это разные вещи (см. «Формат
 * игры» → «Проверка данных перед запуском»). Без предупреждений раздел не добавляется вовсе.
 */
export function formatErrorScreen(errors: EngineError[], warnings: EngineError[]): string {
  const errorsText = formatErrors(errors);
  if (warnings.length === 0) return errorsText;
  return `${errorsText}\n\nПредупреждения:\n${formatErrors(warnings)}`;
}
