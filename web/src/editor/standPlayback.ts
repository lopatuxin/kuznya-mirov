import { useSyncExternalStore } from "react";

export type StandPlaybackState = { phase: number; isPlaying: boolean };

/**
 * Где стоит движение на экране «Анимация»: доля от 0 до 1 и играет ли оно. Доля меняется каждый кадр, поэтому живёт вне
 * окна проекта — на неё подписаны только шкала и ползунки кости с отметками.
 */
export type StandPlayback = {
  getState: () => StandPlaybackState;
  subscribe: (listener: () => void) => () => void;
  setPhase: (phase: number) => void;
  setPlaying: (isPlaying: boolean) => void;
  /** Кадр страницы: играющее движение уходит вперёд на `seconds` по кругу; `length` — длина движения в секундах. */
  advance: (seconds: number, length: number) => void;
  /** Шаг на кадр вперёд (1) или назад (-1) из `frameCount` кадров движения: движение встаёт на паузу, доля идёт по кругу. */
  step: (direction: 1 | -1, frameCount: number) => void;
};

function wrapped(phase: number): number {
  const rest = phase % 1;
  return rest < 0 ? rest + 1 : rest;
}

export function createStandPlayback(): StandPlayback {
  let state: StandPlaybackState = { phase: 0, isPlaying: true };
  const listeners = new Set<() => void>();
  function change(next: StandPlaybackState): void {
    if (next.phase === state.phase && next.isPlaying === state.isPlaying) return;
    state = next;
    for (const listener of listeners) listener();
  }
  return {
    getState: () => state,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    setPhase: (phase) => change({ ...state, phase: Math.min(1, Math.max(0, phase)) }),
    setPlaying: (isPlaying) => change({ ...state, isPlaying }),
    advance: (seconds, length) => {
      if (!state.isPlaying || length <= 0) return;
      change({ ...state, phase: wrapped(state.phase + seconds / length) });
    },
    step: (direction, frameCount) => {
      const count = Math.max(1, frameCount);
      const frame = Math.round(state.phase * count) + direction;
      change({ phase: (((frame % count) + count) % count) / count, isPlaying: false });
    },
  };
}

export function useStandPlayback(playback: StandPlayback): StandPlaybackState {
  return useSyncExternalStore(playback.subscribe, playback.getState);
}
