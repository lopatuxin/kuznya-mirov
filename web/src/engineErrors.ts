export type EngineError = {
  file: string;
  path: string;
  message: string;
  line: number | null;
  column: number | null;
};

function formatPosition(error: EngineError): string {
  if (error.line === null) return "";
  if (error.column === null) return ` (строка ${error.line})`;
  return ` (строка ${error.line}, столбец ${error.column})`;
}

/**
 * Одна строка ошибки или предупреждения движка: файл, место внутри него (пусто, если места ещё
 * нет — как у битого JSON), позиция в файле и текст. Позиция — строка и столбец; у ошибки кода на
 * Lua столбца нет, только строка; `null` в строке значит, что позицию определить не удалось и она
 * уже вшита в текст сообщения. Предупреждения используют этот же тип и этот же формат, что и
 * ошибки (см. «Формат игры» → «Проверка данных перед запуском»).
 */
export function formatError(error: EngineError): string {
  const position = formatPosition(error);
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
