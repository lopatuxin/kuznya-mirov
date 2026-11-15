import { describe, expect, it, vi } from "vitest";
import { playerSettingsKey, restorePlayerSettings, savePlayerSettingsIfChanged, type PlayerSettingsEngine, type SettingsStorage } from "./playerSettings";

function makeStorage(initial: Record<string, string> = {}): SettingsStorage & { data: Map<string, string> } {
  const data = new Map(Object.entries(initial));
  return {
    data,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => {
      data.set(key, value);
    },
  };
}

function makeEngine(changed: boolean, settings = '{"music_volume":40}') {
  return {
    set_player_settings: vi.fn(),
    player_settings: vi.fn(() => settings),
    take_player_settings_changed: vi.fn(() => changed),
  } satisfies PlayerSettingsEngine;
}

describe("playerSettingsKey", () => {
  it("называет ключ по папке игры, чтобы настройки игр не мешались", () => {
    expect(playerSettingsKey("platformer")).toBe("kuznya:settings:platformer");
  });
});

describe("restorePlayerSettings", () => {
  it("отдаёт движку сохранённую строку как есть", () => {
    const engine = makeEngine(false);
    const storage = makeStorage({ "kuznya:settings:platformer": '{"music_volume":20}' });
    restorePlayerSettings(engine, storage, "platformer");
    expect(engine.set_player_settings).toHaveBeenCalledWith('{"music_volume":20}');
  });

  it("не трогает движок, если записи нет или хранилища нет", () => {
    const engine = makeEngine(false);
    restorePlayerSettings(engine, makeStorage(), "platformer");
    restorePlayerSettings(engine, null, "platformer");
    expect(engine.set_player_settings).not.toHaveBeenCalled();
  });

  it("не берёт запись другой игры", () => {
    const engine = makeEngine(false);
    restorePlayerSettings(engine, makeStorage({ "kuznya:settings:snake": "{}" }), "platformer");
    expect(engine.set_player_settings).not.toHaveBeenCalled();
  });

  it("переживает хранилище, которое бросает при чтении", () => {
    const engine = makeEngine(false);
    const storage: SettingsStorage = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {},
    };
    expect(() => restorePlayerSettings(engine, storage, "platformer")).not.toThrow();
    expect(engine.set_player_settings).not.toHaveBeenCalled();
  });
});

describe("savePlayerSettingsIfChanged", () => {
  it("пишет строку движка под ключом игры, когда настройки сменились", () => {
    const storage = makeStorage();
    savePlayerSettingsIfChanged(makeEngine(true), storage, "platformer");
    expect(storage.data.get("kuznya:settings:platformer")).toBe('{"music_volume":40}');
  });

  it("ничего не пишет и не просит строку, пока настройки не менялись", () => {
    const storage = makeStorage();
    const engine = makeEngine(false);
    savePlayerSettingsIfChanged(engine, storage, "platformer");
    expect(storage.data.size).toBe(0);
    expect(engine.player_settings).not.toHaveBeenCalled();
  });

  it("не падает без хранилища и при отказе записи", () => {
    expect(() => savePlayerSettingsIfChanged(makeEngine(true), null, "platformer")).not.toThrow();
    const full: SettingsStorage = {
      getItem: () => null,
      setItem: () => {
        throw new Error("quota");
      },
    };
    expect(() => savePlayerSettingsIfChanged(makeEngine(true), full, "platformer")).not.toThrow();
  });
});
