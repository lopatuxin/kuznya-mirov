import { describe, expect, it, vi } from "vitest";
import { focusSceneWhenGameStarts } from "./sceneInputDom";

describe("focusSceneWhenGameStarts", () => {
  it("партия пошла — фокус на сцене без прокрутки страницы", () => {
    const focus = vi.fn();

    focusSceneWhenGameStarts({ focus }, true);

    expect(focus).toHaveBeenCalledWith({ preventScroll: true });
  });

  it("«Шаг», пауза и повтор партию не включают — фокус не трогается", () => {
    const focus = vi.fn();

    focusSceneWhenGameStarts({ focus }, false);

    expect(focus).not.toHaveBeenCalled();
  });

  it("холста ещё нет — ничего не падает", () => {
    expect(() => focusSceneWhenGameStarts(null, true)).not.toThrow();
  });
});
