import { vi } from "vitest";

/** Проигрыватель, у которого после выдачи источника наступает ровно одно событие загрузки — своё или ошибка, а при `silence` — никакого. */
export class FakeVideo {
  muted = false;
  loop = false;
  playsInline = false;
  preload = "";
  videoWidth: number;
  videoHeight: number;
  currentSource = "";
  pause = vi.fn();
  removeAttribute = vi.fn();
  load = vi.fn();
  private readonly listeners = new Map<string, () => void>();

  constructor(
    private readonly outcome: "loadeddata" | "error" | "silence",
    width = 2,
    height = 4,
  ) {
    this.videoWidth = width;
    this.videoHeight = height;
  }

  get src(): string {
    return this.currentSource;
  }

  set src(value: string) {
    this.currentSource = value;
    queueMicrotask(() => this.listeners.get(this.outcome)?.());
  }

  addEventListener(type: string, listener: () => void): void {
    this.listeners.set(type, listener);
  }
}
