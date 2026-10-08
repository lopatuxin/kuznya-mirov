import type { Engine } from "engine";
import { vi } from "vitest";
import type { SceneWind } from "./sceneWind";

/** Ветер, который поддельный движок ставит повтору, пока тест не поставил другой. */
const DEFAULT_REPLAY_WIND: SceneWind = [9, 9];

export type FakeBattleEngine = Engine & {
  /** Вызовы, у которых важен порядок, — по одной строке на вызов. */
  calls: string[];
  /** Ветер мира глазами движка, как его отдаёт `wind()`; тест меняет его так же, как менял бы перезапуск партии с экрана или событие ветра записи. */
  setWorldWind(wind: SceneWind): void;
  setWind: ReturnType<typeof vi.fn>;
  /** `seek` и `step_back` повтора: тест ставит им, какой ветер движок отдаёт после пересчёта с начала. */
  seekMock: ReturnType<typeof vi.fn>;
  stepBackMock: ReturnType<typeof vi.fn>;
};

type FakeBattleEngineOptions = {
  /** Ветер файла: с него начинается партия, и к нему возвращают «Стоп» и перезапуск партии с экрана. */
  fileWind?: SceneWind;
  replayWind?: SceneWind;
};

/** Движок ровно с теми вызовами, которые читает `useBattleSession`, и ветром мира, который партия, повтор и «Стоп» меняют, как настоящий. */
export function createFakeBattleEngine({ fileWind = [0, 0], replayWind = DEFAULT_REPLAY_WIND }: FakeBattleEngineOptions = {}): FakeBattleEngine {
  const calls: string[] = [];
  let worldWind: [number, number] = [fileWind[0], fileWind[1]];
  const setWorldWind = (wind: SceneWind): void => {
    worldWind = [wind[0], wind[1]];
  };
  const setWind = vi.fn((wind: [number, number]) => {
    calls.push(`set_wind ${JSON.stringify(wind)}`);
    setWorldWind(wind);
    return { ok: true };
  });
  const running = (): { running: true } => ({ running: true });
  const seekMock = vi.fn(running);
  const stepBackMock = vi.fn(running);
  const engine = {
    calls,
    setWorldWind,
    setWind,
    seekMock,
    stepBackMock,
    set_wind: setWind,
    wind: vi.fn(() => [worldWind[0], worldWind[1]]),
    play: vi.fn(() => {
      calls.push("play");
      setWorldWind(fileWind);
    }),
    stop: vi.fn(() => {
      calls.push("stop");
      setWorldWind(fileWind);
    }),
    pause: vi.fn(),
    tick: vi.fn(running),
    step: vi.fn(running),
    seek: seekMock,
    step_back: stepBackMock,
    recording: vi.fn(() => "recording"),
    replay: vi.fn(() => {
      setWorldWind(replayWind);
      return { ok: true };
    }),
    world_objects: vi.fn(() => []),
    scene_point: vi.fn(() => [5, 3]),
    object_at: vi.fn((): number | undefined => undefined),
    object_rect: vi.fn((): { x: number; y: number; width: number; height: number } | undefined => undefined),
    add_object: vi.fn(() => ({ ok: true, id: 1 })),
    object_properties: vi.fn((): Record<string, unknown> | undefined => undefined),
    set_property: vi.fn(() => ({ ok: true })),
    remove_property: vi.fn(() => ({ ok: true })),
    step_report: vi.fn(() => undefined),
    session_messages: vi.fn(() => []),
    current_step: vi.fn(() => 0),
    recording_length: vi.fn(() => 0),
    has_world: vi.fn(() => true),
    step_blocked: vi.fn(() => undefined),
  };
  return engine as unknown as FakeBattleEngine;
}
