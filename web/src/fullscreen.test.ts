import { describe, expect, it, vi } from "vitest";
import { reportFullscreenToEngine, toggleFullscreen, type FullscreenDocument } from "./fullscreen";

type MutableDocument = { fullscreenElement: Element | null; exitFullscreen: ReturnType<typeof vi.fn> } & FullscreenDocument;

function makeDocument(fullscreenElement: Element | null = null) {
  const listeners: Array<() => void> = [];
  const doc = {
    fullscreenElement,
    exitFullscreen: vi.fn(async () => {}),
    addEventListener: (_type: "fullscreenchange", listener: () => void) => {
      listeners.push(listener);
    },
  } as unknown as MutableDocument;
  return { doc, fire: () => listeners.forEach((listener) => listener()) };
}

describe("toggleFullscreen", () => {
  it("разворачивает элемент с холстом, когда окно обычное", async () => {
    const { doc } = makeDocument();
    const target = { requestFullscreen: vi.fn(async () => {}) };
    await toggleFullscreen(target, doc);
    expect(target.requestFullscreen).toHaveBeenCalledOnce();
    expect(doc.exitFullscreen).not.toHaveBeenCalled();
  });

  it("сворачивает окно, когда оно уже развёрнуто", async () => {
    const { doc } = makeDocument({} as Element);
    const target = { requestFullscreen: vi.fn(async () => {}) };
    await toggleFullscreen(target, doc);
    expect(doc.exitFullscreen).toHaveBeenCalledOnce();
    expect(target.requestFullscreen).not.toHaveBeenCalled();
  });

  it("при отказе браузера пишет одну строку в журнал и не бросает", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const { doc } = makeDocument();
    const target = {
      requestFullscreen: vi.fn(async () => {
        throw new TypeError("denied");
      }),
    };
    await expect(toggleFullscreen(target, doc)).resolves.toBeUndefined();
    expect(warn).toHaveBeenCalledTimes(1);
    warn.mockRestore();
  });
});

describe("reportFullscreenToEngine", () => {
  it("сообщает движку, каково окно, при каждой смене", () => {
    const { doc, fire } = makeDocument();
    const engine = { set_fullscreen: vi.fn() };
    reportFullscreenToEngine(engine, doc);

    doc.fullscreenElement = {} as Element;
    fire();
    doc.fullscreenElement = null;
    fire();

    expect(engine.set_fullscreen.mock.calls).toEqual([[true], [false]]);
  });
});
