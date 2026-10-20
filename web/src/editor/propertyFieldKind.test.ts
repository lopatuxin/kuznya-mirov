import { describe, expect, it } from "vitest";
import { propertyFieldKind } from "./propertyFieldKind";

const NO_AUTHOR_PROPERTIES = {};
const IMAGES = ["wall", "cube_i"];

describe("propertyFieldKind", () => {
  it("collides — галочка", () => {
    expect(propertyFieldKind("collides", true, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "checkbox" });
  });

  it("deck — галочка, как collides («Рельеф», требование 46); не булево — текстовое поле", () => {
    expect(propertyFieldKind("deck", true, NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "checkbox" });
    expect(propertyFieldKind("deck", 1, NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "text" });
  });

  it("объявленное свойство автора вида flag — галочка", () => {
    expect(propertyFieldKind("falling", true, { falling: "flag" }, IMAGES, false)).toEqual({ kind: "checkbox" });
  });

  it("camera_follows — галочка (Фаза 11, требование 45)", () => {
    expect(propertyFieldKind("camera_follows", true, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "checkbox" });
  });

  it("flip_x — галочка (Фаза 14, требование 20)", () => {
    expect(propertyFieldKind("flip_x", true, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "checkbox" });
  });

  it("repeat_x — галочка (Фаза 28, требование 22); не булево — текстовое поле", () => {
    expect(propertyFieldKind("repeat_x", true, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "checkbox" });
    expect(propertyFieldKind("repeat_x", 1, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
  });

  it("hold_left, hold_right и hold_jump — галочки (Фаза 37, требование 25); не булево — текстовое поле", () => {
    for (const key of ["hold_left", "hold_right", "hold_jump"]) {
      expect(propertyFieldKind(key, false, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "checkbox" });
    }
    expect(propertyFieldKind("hold_jump", "да", NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
  });

  it("run_speed — число в текстовом поле (Фаза 37)", () => {
    expect(propertyFieldKind("run_speed", 7, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
  });

  it("road — галочка (Фаза 38, требование 34); не булево — текстовое поле", () => {
    expect(propertyFieldKind("road", true, NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "checkbox" });
    expect(propertyFieldKind("road", 1, NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "text" });
  });

  it("road_speed, drive_speed и drive_to — текстовое поле: числа и пара (Фаза 38, требование 34)", () => {
    expect(propertyFieldKind("road_speed", 2, NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "text" });
    expect(propertyFieldKind("drive_speed", 5, NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "text" });
    expect(propertyFieldKind("drive_to", [10, 2], NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "text" });
  });

  it("parallax — число в текстовом поле (Фаза 28, требование 22)", () => {
    expect(propertyFieldKind("parallax", 0.25, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
  });

  it("walk_to, walk_speed и on_click — текстовое поле, как другие пары, числа и keys", () => {
    expect(propertyFieldKind("walk_to", [3, 4], NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
    expect(propertyFieldKind("walk_speed", 4, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
    expect(propertyFieldKind("on_click", [["picked", true]], NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
  });

  it("collides не булев — текстовое поле", () => {
    expect(propertyFieldKind("collides", 1, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
  });

  it("image из files.images — выпадающий список", () => {
    expect(propertyFieldKind("image", "wall", NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "select", options: IMAGES });
  });

  it("картинки нет в files.images — текстовое поле", () => {
    expect(propertyFieldKind("image", "missing", NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
  });

  it("model из объявленных моделей — выпадающий список имён (Фаза 25, требование 24)", () => {
    expect(propertyFieldKind("model", "house", NO_AUTHOR_PROPERTIES, IMAGES, true, ["house", "birch"])).toEqual({ kind: "select", options: ["house", "birch"] });
  });

  it("model, которого нет среди моделей игры, или без моделей — текстовое поле", () => {
    expect(propertyFieldKind("model", "gone", NO_AUTHOR_PROPERTIES, IMAGES, true, ["house"])).toEqual({ kind: "text" });
    expect(propertyFieldKind("model", "house", NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "text" });
  });

  it("road_kind из видов дорог игры — выпадающий список имён (Фаза 43, требование 24)", () => {
    expect(propertyFieldKind("road_kind", "street", NO_AUTHOR_PROPERTIES, IMAGES, true, [], ["street", "avenue"])).toEqual({ kind: "select", options: ["street", "avenue"] });
  });

  it("road_kind, которого нет среди видов игры, или без видов — текстовое поле", () => {
    expect(propertyFieldKind("road_kind", "gone", NO_AUTHOR_PROPERTIES, IMAGES, true, [], ["street"])).toEqual({ kind: "text" });
    expect(propertyFieldKind("road_kind", "street", NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "text" });
  });

  it("rotation из четырёх значений — выпадающий список", () => {
    expect(propertyFieldKind("rotation", 90, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "select", options: [0, 90, 180, 270] });
  });

  it("rotation: 45 — текстовое поле", () => {
    expect(propertyFieldKind("rotation", 45, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
  });

  it("shape из четырёх фигур — выпадающий список (Фаза 15, требование 29)", () => {
    expect(propertyFieldKind("shape", "capsule", NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({
      kind: "select",
      options: ["box", "cylinder", "capsule", "sphere"],
    });
  });

  it("shape не из четырёх — текстовое поле", () => {
    expect(propertyFieldKind("shape", "cone", NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "text" });
  });

  it("height — число в текстовом поле", () => {
    expect(propertyFieldKind("height", 1.8, NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "text" });
  });

  it("rotation в трёхмерной сцене — число, даже если значение из прежних четырёх", () => {
    expect(propertyFieldKind("rotation", 90, NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "text" });
    expect(propertyFieldKind("rotation", 24, NO_AUTHOR_PROPERTIES, IMAGES, true)).toEqual({ kind: "text" });
  });

  it("follow_mouse xy — выпадающий список", () => {
    expect(propertyFieldKind("follow_mouse", "xy", NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "select", options: ["x", "y", "xy"] });
  });

  it("color в виде #rrggbb — палитра", () => {
    expect(propertyFieldKind("color", "#e04040", NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "color" });
  });

  it("цвета эффектов smoke_color, leaf_color и fire_color в виде #rrggbb — палитра, как у color", () => {
    expect(propertyFieldKind("smoke_color", "#a6a6ac", NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "color" });
    expect(propertyFieldKind("leaf_color", "#d9a531", NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "color" });
    expect(propertyFieldKind("fire_color", "#ff8c1a", NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "color" });
    expect(propertyFieldKind("leaf_color", "gold", NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
  });

  it("color не по образцу — текстовое поле", () => {
    expect(propertyFieldKind("color", "red", NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
  });

  it("number/time/timer у автора — текстовое поле", () => {
    expect(propertyFieldKind("score", 3, { score: "number" }, IMAGES, false)).toEqual({ kind: "text" });
  });

  it("объявленное свойство автора вида text — строковое поле без разбора JSON (требование 40)", () => {
    expect(propertyFieldKind("catalogRow", "goblin", { catalogRow: "text" }, IMAGES, false)).toEqual({ kind: "raw-text" });
  });

  it("вид text перекрывает распознавание по имени и значению ключа", () => {
    expect(propertyFieldKind("color", "#e04040", { color: "text" }, IMAGES, false)).toEqual({ kind: "raw-text" });
  });

  it("свойство не из известных — текстовое поле", () => {
    expect(propertyFieldKind("layer", 1, NO_AUTHOR_PROPERTIES, IMAGES, false)).toEqual({ kind: "text" });
  });
});
