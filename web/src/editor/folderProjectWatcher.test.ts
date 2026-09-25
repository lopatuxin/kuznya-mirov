import { describe, expect, it } from "vitest";
import { hasProjectFileChange } from "./folderProjectWatcher";

describe("hasProjectFileChange", () => {
  it("правка scene.json — правка проекта", () => {
    expect(hasProjectFileChange([["scene.json"]])).toBe(true);
  });

  it("появление replays/ и файла записи в ней — не правка проекта (требование 36)", () => {
    expect(hasProjectFileChange([["replays"], ["replays", "2026-03-04-09-05-07.json"]])).toBe(false);
  });

  it("пачка записей, где хоть одна вне replays/ — правка проекта", () => {
    expect(hasProjectFileChange([["replays", "2026-03-04-09-05-07.json"], ["rules.json"]])).toBe(true);
  });

  it("пустая пачка — не правка проекта", () => {
    expect(hasProjectFileChange([])).toBe(false);
  });
});
