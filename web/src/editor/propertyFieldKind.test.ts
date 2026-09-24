import { describe, expect, it } from "vitest";
import { propertyFieldKind } from "./propertyFieldKind";

const NO_AUTHOR_PROPERTIES = {};
const IMAGES = ["wall", "cube_i"];

describe("propertyFieldKind", () => {
  it("collides — галочка", () => {
    expect(propertyFieldKind("collides", true, NO_AUTHOR_PROPERTIES, IMAGES)).toEqual({ kind: "checkbox" });
  });

  it("объявленное свойство автора вида flag — галочка", () => {
    expect(propertyFieldKind("falling", true, { falling: "flag" }, IMAGES)).toEqual({ kind: "checkbox" });
  });

  it("collides не булев — текстовое поле", () => {
    expect(propertyFieldKind("collides", 1, NO_AUTHOR_PROPERTIES, IMAGES)).toEqual({ kind: "text" });
  });

  it("image из files.images — выпадающий список", () => {
    expect(propertyFieldKind("image", "wall", NO_AUTHOR_PROPERTIES, IMAGES)).toEqual({ kind: "select", options: IMAGES });
  });

  it("картинки нет в files.images — текстовое поле", () => {
    expect(propertyFieldKind("image", "missing", NO_AUTHOR_PROPERTIES, IMAGES)).toEqual({ kind: "text" });
  });

  it("rotation из четырёх значений — выпадающий список", () => {
    expect(propertyFieldKind("rotation", 90, NO_AUTHOR_PROPERTIES, IMAGES)).toEqual({ kind: "select", options: [0, 90, 180, 270] });
  });

  it("rotation: 45 — текстовое поле", () => {
    expect(propertyFieldKind("rotation", 45, NO_AUTHOR_PROPERTIES, IMAGES)).toEqual({ kind: "text" });
  });

  it("follow_mouse xy — выпадающий список", () => {
    expect(propertyFieldKind("follow_mouse", "xy", NO_AUTHOR_PROPERTIES, IMAGES)).toEqual({ kind: "select", options: ["x", "y", "xy"] });
  });

  it("color в виде #rrggbb — палитра", () => {
    expect(propertyFieldKind("color", "#e04040", NO_AUTHOR_PROPERTIES, IMAGES)).toEqual({ kind: "color" });
  });

  it("color не по образцу — текстовое поле", () => {
    expect(propertyFieldKind("color", "red", NO_AUTHOR_PROPERTIES, IMAGES)).toEqual({ kind: "text" });
  });

  it("number/time/timer у автора — текстовое поле", () => {
    expect(propertyFieldKind("score", 3, { score: "number" }, IMAGES)).toEqual({ kind: "text" });
  });

  it("свойство не из известных — текстовое поле", () => {
    expect(propertyFieldKind("layer", 1, NO_AUTHOR_PROPERTIES, IMAGES)).toEqual({ kind: "text" });
  });
});
