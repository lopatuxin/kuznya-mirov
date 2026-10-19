import { describe, expect, it } from "vitest";
import { IMAGE_DRAG_TYPE, MODEL_DRAG_TYPE, PARTICLES_DRAG_TYPE, resolveSceneDropEffect } from "./imageDrag";

describe("resolveSceneDropEffect", () => {
  it("картинку из вкладки сцена принимает, когда можно, и показывает запрет, когда нельзя", () => {
    expect(resolveSceneDropEffect([IMAGE_DRAG_TYPE], true, false)).toBe("copy");
    expect(resolveSceneDropEffect([IMAGE_DRAG_TYPE], false, false)).toBe("none");
  });

  it("карточка частиц из вкладки принимается так же, как картинка", () => {
    expect(resolveSceneDropEffect([PARTICLES_DRAG_TYPE], true, false)).toBe("copy");
    expect(resolveSceneDropEffect([PARTICLES_DRAG_TYPE], false, false)).toBe("none");
  });

  it("имя модели принимает только трёхмерная сцена, картинка в ней показывает запрет", () => {
    expect(resolveSceneDropEffect([MODEL_DRAG_TYPE], false, true)).toBe("copy");
    expect(resolveSceneDropEffect([MODEL_DRAG_TYPE], true, false)).toBe("none");
    expect(resolveSceneDropEffect([IMAGE_DRAG_TYPE], false, true)).toBe("none");
  });

  it("чужие перетаскивания (файл с диска, текст) сцена не трогает", () => {
    expect(resolveSceneDropEffect(["Files"], true, false)).toBeNull();
    expect(resolveSceneDropEffect(["text/plain"], false, false)).toBeNull();
    expect(resolveSceneDropEffect([], true, false)).toBeNull();
  });
});
