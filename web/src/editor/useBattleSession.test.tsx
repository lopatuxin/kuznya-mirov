// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createFakeBattleEngine, type FakeBattleEngine } from "./fakeBattleEngine";
import { writeProjectFile } from "./projectFileWriter";
import { useBattleSession, type BattleSessionState } from "./useBattleSession";

vi.mock("./projectFileWriter", () => ({ writeProjectFile: vi.fn(() => Promise.resolve({ ok: true })) }));

const FILE_WIND = [1.5, 0] as const;

function renderSession(engine: FakeBattleEngine): { current: BattleSessionState } {
  const { result } = renderHook(() =>
    useBattleSession({
      engine,
      memory: null,
      source: { kind: "listed", name: "qa-wind" },
      sceneAvailable: true,
      loadedSounds: [],
      musicTracks: [],
      audioContext: null,
      audioElement: null,
      sceneObjectCount: 0,
      editSelectedIndex: null,
      onEditSelectionChange: () => {},
      hasQueuedReload: false,
      setReloadGateOpen: () => {},
    }),
  );
  return result;
}

describe("useBattleSession: ветер живой игры", () => {
  let engine: FakeBattleEngine;
  let session: { current: BattleSessionState };

  beforeEach(() => {
    vi.stubGlobal("requestAnimationFrame", () => 0);
    vi.stubGlobal("cancelAnimationFrame", () => {});
    vi.mocked(writeProjectFile).mockClear();
    engine = createFakeBattleEngine({ fileWind: FILE_WIND, replayWind: [-3, 1] });
    session = renderSession(engine);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  function startBattle(): void {
    act(() => session.current.play());
  }

  it("в партии поля показывают ветер движка — на «Запуске» это ветер файла", () => {
    startBattle();

    expect(session.current.liveWind).toEqual(FILE_WIND);
  });

  it("правка на ходу зовёт set_wind, показывает новый ветер сразу и не пишет файл", () => {
    startBattle();

    let error: string | undefined;
    act(() => {
      error = session.current.setLiveWind([-2, 0.5]);
    });

    expect(error).toBeUndefined();
    expect(engine.calls).toEqual(["play", "set_wind [-2,0.5]"]);
    expect(session.current.liveWind).toEqual([-2, 0.5]);
    expect(session.current.canUndoLiveEdit).toBe(true);
    expect(writeProjectFile).not.toHaveBeenCalled();
  });

  it("Ctrl+Z возвращает прежний ветер тем же вызовом движка", () => {
    startBattle();
    act(() => session.current.setLiveWind([-2, 0.5]));
    act(() => session.current.setLiveWind([4, 4]));

    act(() => session.current.undoLiveEdit());
    expect(engine.calls.at(-1)).toBe("set_wind [-2,0.5]");
    expect(session.current.liveWind).toEqual([-2, 0.5]);

    act(() => session.current.undoLiveEdit());
    expect(engine.calls.at(-1)).toBe("set_wind [1.5,0]");
    expect(session.current.liveWind).toEqual(FILE_WIND);
    expect(session.current.canUndoLiveEdit).toBe(false);
    expect(writeProjectFile).not.toHaveBeenCalled();
  });

  it("отмена возвращает ветер, каким его видел движок, а не последнюю правку из истории", () => {
    startBattle();
    act(() => session.current.setLiveWind([-2, 0]));
    act(() => engine.setWorldWind([5, 5]));
    act(() => session.current.setLiveWind([4, 4]));

    act(() => session.current.undoLiveEdit());

    expect(engine.calls.at(-1)).toBe("set_wind [5,5]");
  });

  it("движок не принял ветер — возвращается его текст, ветер и история прежние", () => {
    startBattle();
    engine.setWind.mockReturnValueOnce({ ok: false, error: "ветер не пара конечных чисел" });

    let error: string | undefined;
    act(() => {
      error = session.current.setLiveWind([Number.NaN, 0]);
    });

    expect(error).toBe("ветер не пара конечных чисел");
    expect(session.current.liveWind).toEqual(FILE_WIND);
    expect(session.current.canUndoLiveEdit).toBe(false);
  });

  it("на паузе ветер ставится так же, как на ходу", () => {
    startBattle();
    act(() => session.current.pauseOrResume());

    act(() => session.current.setLiveWind([2, 0]));

    expect(session.current.isRunning).toBe(false);
    expect(engine.calls).toEqual(["play", "set_wind [2,0]"]);
    expect(session.current.liveWind).toEqual([2, 0]);
  });

  it("вне партии правка на ходу ничего не делает", () => {
    let error: string | undefined;
    act(() => {
      error = session.current.setLiveWind([2, 0]);
    });

    expect(error).toBeUndefined();
    expect(engine.setWind).not.toHaveBeenCalled();
  });

  it("ветер поменял сам мир — например, перезапуск партии с экрана вернул ветер файла — поля показывают его, а не последнюю правку", () => {
    startBattle();
    act(() => session.current.setLiveWind([7, 0]));

    act(() => engine.setWorldWind([3, 3]));
    act(() => session.current.step());
    expect(session.current.liveWind).toEqual([3, 3]);

    act(() => engine.setWorldWind(FILE_WIND));
    act(() => session.current.step());
    expect(session.current.liveWind).toEqual(FILE_WIND);
  });

  it("в повторе поля показывают ветер записи, а не файла, и он меняется со «Шагом назад» и шкалой", () => {
    startBattle();
    act(() => session.current.startReplay());
    expect(session.current.mode).toBe("replay");
    expect(session.current.liveWind).toEqual([-3, 1]);

    engine.seekMock.mockImplementationOnce(() => {
      engine.setWorldWind([6, 0]);
      return { running: true };
    });
    act(() => session.current.seek(40));
    expect(session.current.liveWind).toEqual([6, 0]);

    engine.stepBackMock.mockImplementationOnce(() => {
      engine.setWorldWind([-3, 1]);
      return { running: true };
    });
    act(() => session.current.stepBack());
    expect(session.current.liveWind).toEqual([-3, 1]);
  });

  it("в повторе правка ветра не доходит до движка", () => {
    startBattle();
    act(() => session.current.startReplay());

    act(() => session.current.setLiveWind([8, 8]));

    expect(engine.setWind).not.toHaveBeenCalled();
    expect(session.current.liveWind).toEqual([-3, 1]);
  });
});

describe("useBattleSession: свойства частиц на ходу", () => {
  let engine: FakeBattleEngine;
  let session: { current: BattleSessionState };

  beforeEach(() => {
    vi.stubGlobal("requestAnimationFrame", () => 0);
    vi.stubGlobal("cancelAnimationFrame", () => {});
    vi.mocked(writeProjectFile).mockClear();
    engine = createFakeBattleEngine();
    vi.mocked(engine.world_objects).mockReturnValue([{ id: 3, generation: 1, name: null }]);
    session = renderSession(engine);
    act(() => session.current.play());
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("поле уже ставило значение миру на лету: в отмену идёт прежнее из original, а не то, что движок отдаёт сейчас", () => {
    vi.mocked(engine.object_properties).mockReturnValue({ smoke: 0.9 });

    act(() => {
      session.current.setLiveProperty(3, "smoke", 0.9, { hadKey: true, previous: 0.5 });
    });
    act(() => session.current.undoLiveEdit());

    expect(engine.set_property).toHaveBeenLastCalledWith(3, "smoke", 0.5);
    expect(writeProjectFile).not.toHaveBeenCalled();
  });

  it("свойства до жеста не было — отмена его снимает", () => {
    vi.mocked(engine.object_properties).mockReturnValue({ sparks_reach: 2 });

    act(() => {
      session.current.setLiveProperty(3, "sparks_reach", 2, { hadKey: false, previous: undefined });
    });
    act(() => session.current.undoLiveEdit());

    expect(engine.remove_property).toHaveBeenLastCalledWith(3, "sparks_reach");
  });

  it("«Убрать»: свойства объекта снимаются одной записью, одна отмена возвращает все", () => {
    vi.mocked(engine.object_properties).mockReturnValue({ position: [1, 1], sparks: 0.5, sparks_reach: 2 });

    act(() => session.current.removeLiveProperties(3, ["sparks", "sparks_reach", "sparks_spread"]));

    expect(engine.remove_property).toHaveBeenCalledTimes(2);
    expect(engine.remove_property).not.toHaveBeenCalledWith(3, "sparks_spread");

    act(() => session.current.undoLiveEdit());

    expect(engine.set_property).toHaveBeenCalledWith(3, "sparks", 0.5);
    expect(engine.set_property).toHaveBeenCalledWith(3, "sparks_reach", 2);
    expect(session.current.canUndoLiveEdit).toBe(false);
  });

  it("снимать нечего — записи отмены нет", () => {
    vi.mocked(engine.object_properties).mockReturnValue({ position: [1, 1] });

    act(() => session.current.removeLiveProperties(3, ["sparks"]));

    expect(session.current.canUndoLiveEdit).toBe(false);
  });
});
