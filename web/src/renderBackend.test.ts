import { afterEach, describe, expect, it } from "vitest";
import { hasWebgl2Flag, hideWebGpuIfRequested, logEngineBackend } from "./renderBackend";

describe("hasWebgl2Flag", () => {
  it("находит признак среди других параметров", () => {
    expect(hasWebgl2Flag("?game=rpg&webgl2")).toBe(true);
  });

  it("находит признак сам по себе", () => {
    expect(hasWebgl2Flag("?webgl2")).toBe(true);
  });

  it("без признака — нет", () => {
    expect(hasWebgl2Flag("?game=rpg")).toBe(false);
    expect(hasWebgl2Flag("")).toBe(false);
  });
});

describe("hideWebGpuIfRequested", () => {
  const originalGpu = (navigator as unknown as { gpu?: unknown }).gpu;

  afterEach(() => {
    Object.defineProperty(navigator, "gpu", { value: originalGpu, configurable: true });
  });

  it("с признаком webgl2 прячет navigator.gpu", () => {
    Object.defineProperty(navigator, "gpu", { value: {}, configurable: true });
    hideWebGpuIfRequested("?game=rpg&webgl2");
    expect((navigator as unknown as { gpu?: unknown }).gpu).toBeUndefined();
  });

  it("без признака navigator.gpu не трогает", () => {
    Object.defineProperty(navigator, "gpu", { value: {}, configurable: true });
    hideWebGpuIfRequested("?game=rpg");
    expect((navigator as unknown as { gpu?: unknown }).gpu).toEqual({});
  });
});

describe("logEngineBackend", () => {
  it("пишет в консоль способ рисования", () => {
    const messages: string[] = [];
    const logSpy = (message: string): void => {
      messages.push(message);
    };
    const originalLog = console.log;
    console.log = logSpy;
    try {
      logEngineBackend({ backend: () => "webgl2" });
    } finally {
      console.log = originalLog;
    }
    expect(messages).toEqual(["движок рисует через webgl2"]);
  });
});
