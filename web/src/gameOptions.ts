import { parseGameDisplayName, parseGamesIndex } from "./gamesIndex";

export type GameOption = { id: string; name: string };
export type GameLoadFailure = { id: string; reason: string };
export type GameOptionsResult = { options: GameOption[]; failures: GameLoadFailure[] };

/**
 * Обёртка над `fetch`, которая сводит сетевую ошибку и HTTP-статус вне 2xx к одному и тому же
 * `null` — вызывающему коду всё равно, что именно пошло не так, важно только, что файл недоступен.
 */
export async function fetchText(url: string): Promise<string | null> {
  try {
    const response = await fetch(url);
    return response.ok ? await response.text() : null;
  } catch {
    return null;
  }
}

/**
 * Список игр для экрана выбора. Недоступный или повреждённый `game.json` одной игры — не повод
 * скрывать остальные: `Promise.allSettled` собирает исправные игры в `options`, а причины отказа
 * сломанных — в `failures`, отдельно по каждой. Сам `games/index.json` остаётся фатальной ошибкой:
 * без него нет и списка папок, которые можно было бы попробовать прочитать.
 */
export async function loadGameOptions(): Promise<GameOptionsResult> {
  const indexText = await fetchText("/games/index.json");
  if (indexText === null) {
    throw new Error("Не удалось получить список игр по адресу /games/index.json.");
  }
  const ids = parseGamesIndex(indexText);

  const settled = await Promise.allSettled(
    ids.map(async (id): Promise<GameOption> => {
      const gameJsonText = await fetchText(`/games/${id}/game.json`);
      if (gameJsonText === null) {
        throw new Error(`не удалось получить game.json по адресу /games/${id}/game.json`);
      }
      try {
        return { id, name: parseGameDisplayName(gameJsonText) };
      } catch (error) {
        const reason = error instanceof Error ? error.message : String(error);
        throw new Error(`game.json повреждён: ${reason}`);
      }
    }),
  );

  const options: GameOption[] = [];
  const failures: GameLoadFailure[] = [];
  settled.forEach((result, index) => {
    if (result.status === "fulfilled") {
      options.push(result.value);
    } else {
      const reason = result.reason instanceof Error ? result.reason.message : String(result.reason);
      failures.push({ id: ids[index] as string, reason });
    }
  });
  return { options, failures };
}
