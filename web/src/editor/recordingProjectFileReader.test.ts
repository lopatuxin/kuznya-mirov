import { describe, expect, it } from "vitest";
import type { ProjectFileReader } from "../projectLoader";
import { createRecordingProjectFileReader } from "./recordingProjectFileReader";

function stubReader(): ProjectFileReader {
  return {
    readText: async () => "text",
    readBinary: async () => new Uint8Array(),
  };
}

describe("createRecordingProjectFileReader", () => {
  it("запоминает пути readText и readBinary без повторов", async () => {
    const recording = createRecordingProjectFileReader(stubReader());

    await recording.reader.readText("game.json");
    await recording.reader.readBinary("fonts/Rubik.ttf");
    await recording.reader.readText("game.json");

    expect(recording.getReadPaths()).toEqual(["game.json", "fonts/Rubik.ttf"]);
  });

  it("reset очищает список перед новой загрузкой", async () => {
    const recording = createRecordingProjectFileReader(stubReader());
    await recording.reader.readText("game.json");

    recording.reset();
    await recording.reader.readText("scene.json");

    expect(recording.getReadPaths()).toEqual(["scene.json"]);
  });

  it("отдаёт значения исходной читалки без изменений", async () => {
    const recording = createRecordingProjectFileReader(stubReader());

    expect(await recording.reader.readText("game.json")).toBe("text");
  });
});
