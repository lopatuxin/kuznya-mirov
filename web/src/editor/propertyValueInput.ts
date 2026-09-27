/**
 * Разбирает текст, который автор набрал в текстовое поле значения, — «Редактор», требование 13:
 * JSON, а не разбирается — берётся строкой (`#e04040` → `"#e04040"`, `abc` у `layer` → строка
 * `"abc"`, а проверка движком уже назовёт ошибку вида значения). Свойство автора вида `text` не
 * проходит через JSON вовсе — набранное `123` остаётся строкой `"123"` («Таблицы данных»,
 * требование 40).
 */
export function parsePropertyValueInput(text: string, isRawText = false): unknown {
  if (isRawText) return text;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}
