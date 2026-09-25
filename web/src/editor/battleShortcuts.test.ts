import { describe, expect, it } from "vitest";
import { isPauseResumeShortcut, isPlayStopShortcut, isReplaySeekShortcut, isStepShortcut } from "./battleShortcuts";

describe("isPlayStopShortcut", () => {
  it("Ctrl+P — да", () => {
    expect(isPlayStopShortcut({ code: "KeyP", ctrlKey: true, shiftKey: false, altKey: false })).toBe(true);
  });

  it("Ctrl+Shift+P — нет, это пауза", () => {
    expect(isPlayStopShortcut({ code: "KeyP", ctrlKey: true, shiftKey: true, altKey: false })).toBe(false);
  });

  it("P без Ctrl — нет", () => {
    expect(isPlayStopShortcut({ code: "KeyP", ctrlKey: false, shiftKey: false, altKey: false })).toBe(false);
  });
});

describe("isPauseResumeShortcut", () => {
  it("Ctrl+Shift+P — да", () => {
    expect(isPauseResumeShortcut({ code: "KeyP", ctrlKey: true, shiftKey: true, altKey: false })).toBe(true);
  });

  it("Ctrl+P — нет", () => {
    expect(isPauseResumeShortcut({ code: "KeyP", ctrlKey: true, shiftKey: false, altKey: false })).toBe(false);
  });
});

describe("isStepShortcut", () => {
  it("Ctrl+Alt+P — да", () => {
    expect(isStepShortcut({ code: "KeyP", ctrlKey: true, shiftKey: false, altKey: true })).toBe(true);
  });

  it("Ctrl+Shift+Alt+P — нет", () => {
    expect(isStepShortcut({ code: "KeyP", ctrlKey: true, shiftKey: true, altKey: true })).toBe(false);
  });
});

describe("isReplaySeekShortcut", () => {
  it("← — назад", () => {
    expect(isReplaySeekShortcut({ code: "ArrowLeft", ctrlKey: false, shiftKey: false, altKey: false })).toBe("back");
  });

  it("→ — вперёд", () => {
    expect(isReplaySeekShortcut({ code: "ArrowRight", ctrlKey: false, shiftKey: false, altKey: false })).toBe("forward");
  });

  it("Ctrl+← — не сочетание шкалы", () => {
    expect(isReplaySeekShortcut({ code: "ArrowLeft", ctrlKey: true, shiftKey: false, altKey: false })).toBe(null);
  });

  it("другая клавиша — null", () => {
    expect(isReplaySeekShortcut({ code: "KeyA", ctrlKey: false, shiftKey: false, altKey: false })).toBe(null);
  });
});
