import { describe, expect, it } from "vitest";
import { resolveProjectName } from "./projectName";

describe("resolveProjectName", () => {
  it("отдаёт absent без параметра", () => {
    expect(resolveProjectName("")).toEqual({ status: "absent" });
  });

  it("отдаёт valid для допустимого имени", () => {
    expect(resolveProjectName("?project=tetris")).toEqual({ status: "valid", name: "tetris" });
  });

  it("отдаёт invalid для имени с недопустимыми символами", () => {
    expect(resolveProjectName("?project=../secret")).toEqual({ status: "invalid", value: "../secret" });
  });
});
