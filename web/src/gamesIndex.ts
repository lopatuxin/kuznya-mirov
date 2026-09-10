import { isValidGameName } from "./gameName";

/**
 * Разбирает `games/index.json` — список имён папок игр (`["snake","arkanoid"]`) для экрана выбора.
 * Бросает исключение с русским текстом на любой проблеме: страница показывает его как есть,
 * так же как она уже показывает ошибки чтения `game.json`. Имя проверяется тем же правилом
 * (`isValidGameName`), что и `?game=` в адресе, — иначе кнопка на экране выбора вела бы на ссылку,
 * которую сама страница потом отвергла бы как недопустимую.
 */
export function parseGamesIndex(text: string): string[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new Error("games/index.json содержит недействительный JSON.");
  }
  if (!Array.isArray(parsed) || parsed.length === 0) {
    throw new Error("games/index.json пуст или не является списком игр.");
  }
  if (!parsed.every((entry): entry is string => typeof entry === "string" && isValidGameName(entry))) {
    throw new Error(
      "games/index.json содержит имя папки игры с недопустимыми символами — разрешены только латинские буквы, цифры, «_» и «-».",
    );
  }
  return parsed;
}

/**
 * Достаёт человеческое название игры из поля `name` её `game.json` — экран выбора не дублирует
 * названия отдельно от игр, а читает их напрямую оттуда.
 */
export function parseGameDisplayName(gameJsonText: string): string {
  let parsed: unknown;
  try {
    parsed = JSON.parse(gameJsonText);
  } catch {
    throw new Error("не удалось разобрать JSON.");
  }
  const name = (parsed as { name?: unknown } | null)?.name;
  if (typeof name !== "string" || name.length === 0) {
    throw new Error("отсутствует поле name.");
  }
  return name;
}
