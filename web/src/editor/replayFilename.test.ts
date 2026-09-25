import { describe, expect, it } from "vitest";
import { formatReplayTimestamp, pickAvailableReplayFileName, replayFileNameForAttempt } from "./replayFilename";

describe("formatReplayTimestamp", () => {
  it("собирает ГГГГ-ММ-ДД-ЧЧ-ММ-СС по местному времени, с ведущими нулями", () => {
    const startedAt = new Date(2026, 2, 4, 9, 5, 7);
    expect(formatReplayTimestamp(startedAt)).toBe("2026-03-04-09-05-07");
  });
});

describe("replayFileNameForAttempt", () => {
  it("первая попытка — без суффикса", () => {
    expect(replayFileNameForAttempt("2026-03-04-09-05-07", 1)).toBe("2026-03-04-09-05-07.json");
  });

  it("вторая попытка — суффикс -2", () => {
    expect(replayFileNameForAttempt("2026-03-04-09-05-07", 2)).toBe("2026-03-04-09-05-07-2.json");
  });
});

describe("pickAvailableReplayFileName", () => {
  it("свободное имя — без суффикса", async () => {
    const name = await pickAvailableReplayFileName("2026-03-04-09-05-07", async () => false);
    expect(name).toBe("2026-03-04-09-05-07.json");
  });

  it("занято — вторая запись получает суффикс -2 (требование 34)", async () => {
    const taken = new Set(["2026-03-04-09-05-07.json"]);
    const name = await pickAvailableReplayFileName("2026-03-04-09-05-07", async (fileName) => taken.has(fileName));
    expect(name).toBe("2026-03-04-09-05-07-2.json");
  });

  it("заняты первые два — третья попытка", async () => {
    const taken = new Set(["2026-03-04-09-05-07.json", "2026-03-04-09-05-07-2.json"]);
    const name = await pickAvailableReplayFileName("2026-03-04-09-05-07", async (fileName) => taken.has(fileName));
    expect(name).toBe("2026-03-04-09-05-07-3.json");
  });
});
