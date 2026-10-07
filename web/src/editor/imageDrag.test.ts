import { describe, expect, it } from "vitest";
import { IMAGE_DRAG_TYPE, PARTICLES_DRAG_TYPE, resolveSceneDropEffect } from "./imageDrag";

describe("resolveSceneDropEffect", () => {
  it("картинку из вкладки сцена принимает, когда можно, и показывает запрет, когда нельзя", () => {
    expect(resolveSceneDropEffect([IMAGE_DRAG_TYPE], true)).toBe("copy");
    expect(resolveSceneDropEffect([IMAGE_DRAG_TYPE], false)).toBe("none");
  });

  it("вид частиц из вкладки принимается так же, как картинка", () => {
    expect(resolveSceneDropEffect([PARTICLES_DRAG_TYPE], true)).toBe("copy");
    expect(resolveSceneDropEffect([PARTICLES_DRAG_TYPE], false)).toBe("none");
  });

  it("чужие перетаскивания (файл с диска, текст) сцена не трогает", () => {
    expect(resolveSceneDropEffect(["Files"], true)).toBeNull();
    expect(resolveSceneDropEffect(["text/plain"], false)).toBeNull();
    expect(resolveSceneDropEffect([], true)).toBeNull();
  });
});
