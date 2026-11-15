// «Экраны и состояние» → «Настройки игрока»: хранит настройки игрока страница, а не движок — в хранилище
// браузера под именем папки игры. Строку JSON страница не разбирает: что в ней подходит игре, решает движок.

export type SettingsStorage = Pick<Storage, "getItem" | "setItem">;

export type PlayerSettingsEngine = {
  set_player_settings(json: string): void;
  player_settings(): string;
  take_player_settings_changed(): boolean;
};

export function playerSettingsKey(gameName: string): string {
  return `kuznya:settings:${gameName}`;
}

/** Хранилище бывает недоступно (закрытое окно, запрещённые данные сайта): тогда игра идёт без памяти. */
export function browserStorage(): SettingsStorage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

/** До первого `tick`: отдаёт движку сохранённое. Нет записи или хранилища — движок остаётся с умолчаниями. */
export function restorePlayerSettings(engine: PlayerSettingsEngine, storage: SettingsStorage | null, gameName: string): void {
  let stored: string | null = null;
  try {
    stored = storage?.getItem(playerSettingsKey(gameName)) ?? null;
  } catch {
    return;
  }
  if (stored !== null) engine.set_player_settings(stored);
}

/** После `tick`: пишет настройки, только если движок сказал, что они сменились. */
export function savePlayerSettingsIfChanged(engine: PlayerSettingsEngine, storage: SettingsStorage | null, gameName: string): void {
  if (!engine.take_player_settings_changed()) return;
  try {
    storage?.setItem(playerSettingsKey(gameName), engine.player_settings());
  } catch {
    // Переполненное или запрещённое хранилище не должно останавливать игру: настройки живут до перезагрузки.
  }
}
