import { describe, expect, it } from "vitest";
import { resolveFolderPathSegments } from "./folderProjectFileReader";

describe("resolveFolderPathSegments", () => {
  it("разбирает файл в подпапке на части", () => {
    expect(resolveFolderPathSegments("fonts/Rubik.ttf")).toEqual(["fonts", "Rubik.ttf"]);
  });

  it("разбирает файл в корне на одну часть", () => {
    expect(resolveFolderPathSegments("game.json")).toEqual(["game.json"]);
  });

  it("отвергает путь с ..", () => {
    expect(resolveFolderPathSegments("../secret.json")).toBe(null);
  });

  it("отвергает путь с .. в середине", () => {
    expect(resolveFolderPathSegments("fonts/../game.json")).toBe(null);
  });

  it("отвергает пустую часть", () => {
    expect(resolveFolderPathSegments("fonts//Rubik.ttf")).toBe(null);
  });

  it("отвергает абсолютный путь", () => {
    expect(resolveFolderPathSegments("/etc/passwd")).toBe(null);
  });

  it("отвергает пустой путь", () => {
    expect(resolveFolderPathSegments("")).toBe(null);
  });
});
