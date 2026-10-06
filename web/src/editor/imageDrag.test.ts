import { describe, expect, it } from "vitest";
import { IMAGE_DRAG_TYPE, resolveImageDropEffect } from "./imageDrag";

describe("resolveImageDropEffect", () => {
  it("картинку из вкладки сцена принимает, когда можно, и показывает запрет, когда нельзя", () => {
    expect(resolveImageDropEffect([IMAGE_DRAG_TYPE], true)).toBe("copy");
    expect(resolveImageDropEffect([IMAGE_DRAG_TYPE], false)).toBe("none");
  });

  it("чужие перетаскивания (файл с диска, текст) сцена не трогает", () => {
    expect(resolveImageDropEffect(["Files"], true)).toBeNull();
    expect(resolveImageDropEffect(["text/plain"], false)).toBeNull();
    expect(resolveImageDropEffect([], true)).toBeNull();
  });
});
