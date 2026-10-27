import { describe, expect, it, vi } from "vitest";
import { createStandPlayback } from "./standPlayback";

describe("доля движения на экране «Анимация»", () => {
  it("играющее движение идёт вперёд по кругу, на паузе стоит", () => {
    const playback = createStandPlayback();

    playback.advance(0.25, 1);
    expect(playback.getState().phase).toBeCloseTo(0.25, 6);
    playback.advance(1, 1);
    expect(playback.getState().phase).toBeCloseTo(0.25, 6);

    playback.setPlaying(false);
    playback.advance(0.5, 1);
    expect(playback.getState().phase).toBeCloseTo(0.25, 6);
  });

  it("шаг на кадр встаёт на паузу и идёт по кругу в обе стороны", () => {
    const playback = createStandPlayback();

    playback.step(-1, 4);
    expect(playback.getState()).toEqual({ phase: 0.75, isPlaying: false });
    playback.step(1, 4);
    playback.step(1, 4);
    expect(playback.getState().phase).toBe(0.25);
  });

  it("доля перетаскивания не выходит за 0 и 1, подписчик слышит только настоящие перемены", () => {
    const playback = createStandPlayback();
    const listener = vi.fn();
    const unsubscribe = playback.subscribe(listener);

    playback.setPhase(1.7);
    playback.setPhase(1.7);
    expect(playback.getState().phase).toBe(1);
    expect(listener).toHaveBeenCalledTimes(1);

    unsubscribe();
    playback.setPhase(0.2);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});
