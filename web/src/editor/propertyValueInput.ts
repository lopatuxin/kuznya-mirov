/**
 * Разбирает текст, который автор набрал в текстовое поле значения, — «Редактор», требование 13:
 * JSON, а не разбирается — берётся строкой (`#e04040` → `"#e04040"`, `abc` у `layer` → строка
 * `"abc"`, а проверка движком уже назовёт ошибку вида значения).
 */
export function parsePropertyValueInput(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}
