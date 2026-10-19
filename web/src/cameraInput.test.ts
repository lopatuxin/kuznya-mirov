// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { attachCameraMouse, isPanButton, wheelDeltaPoints, type CameraMouseEngine } from "./cameraInput";

describe("isPanButton", () => {
  it("средняя и правая тащат карту, левая — нет", () => {
    expect(isPanButton(1)).toBe(true);
    expect(isPanButton(2)).toBe(true);
    expect(isPanButton(0)).toBe(false);
  });
});

describe("wheelDeltaPoints", () => {
  it("пиксели идут как есть: щелчок Chrome — 100", () => {
    expect(wheelDeltaPoints(-100, 0)).toBe(-100);
    expect(wheelDeltaPoints(12, 0)).toBe(12);
  });

  it("щелчок Firefox — три строки — тоже 100 точек, страница — 100", () => {
    expect(wheelDeltaPoints(3, 1)).toBeCloseTo(100, 9);
    expect(wheelDeltaPoints(-1, 2)).toBe(-100);
  });
});

describe("attachCameraMouse", () => {
  const calls: string[] = [];
  const engine: CameraMouseEngine = {
    mouse_move: (x, y) => void calls.push(`move ${x} ${y}`),
    mouse_wheel: (deltaY) => void calls.push(`wheel ${deltaY}`),
    pan_down: () => void calls.push("pan_down"),
    pan_up: () => void calls.push("pan_up"),
  };
  const canvas = document.createElement("canvas");
  document.body.append(canvas);
  attachCameraMouse(engine, canvas);

  afterEach(() => {
    calls.length = 0;
  });

  it("колесо: сначала точка указателя, потом прокрутка, страница не прокручивается", () => {
    const event = new WheelEvent("wheel", { deltaY: -100, clientX: 300, clientY: 200, bubbles: true, cancelable: true });
    canvas.dispatchEvent(event);

    expect(calls).toEqual(["move 300 200", "wheel -100"]);
    expect(event.defaultPrevented).toBe(true);
  });

  it("правая и средняя кнопки над холстом хватают карту, отпускание в окне — отпускает", () => {
    for (const button of [1, 2]) {
      const down = new MouseEvent("mousedown", { button, clientX: 40, clientY: 50, bubbles: true, cancelable: true });
      canvas.dispatchEvent(down);
      window.dispatchEvent(new MouseEvent("mouseup", { button }));

      expect(down.defaultPrevented).toBe(true);
    }

    expect(calls).toEqual(["move 40 50", "pan_down", "pan_up", "move 40 50", "pan_down", "pan_up"]);
  });

  it("левая кнопка карту не хватает и не отпускает", () => {
    canvas.dispatchEvent(new MouseEvent("mousedown", { button: 0, bubbles: true }));
    window.dispatchEvent(new MouseEvent("mouseup", { button: 0 }));

    expect(calls).toEqual([]);
  });

  it("меню правой кнопки над холстом не открывается", () => {
    const menu = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    canvas.dispatchEvent(menu);

    expect(menu.defaultPrevented).toBe(true);
  });

  it("кнопка над другим элементом карту не хватает", () => {
    const panel = document.createElement("div");
    document.body.append(panel);
    panel.dispatchEvent(new MouseEvent("mousedown", { button: 2, bubbles: true }));

    expect(calls).toEqual([]);
  });
});
