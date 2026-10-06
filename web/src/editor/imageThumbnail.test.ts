import { describe, expect, it } from "vitest";
import { thumbnailRect } from "./imageThumbnail";

describe("thumbnailRect", () => {
  it("широкий кадр вписан по ширине и стоит по середине по высоте", () => {
    expect(thumbnailRect([128, 64])).toEqual({ x: 0, y: 16, width: 64, height: 32 });
  });

  it("высокий кадр вписан по высоте", () => {
    expect(thumbnailRect([32, 128])).toEqual({ x: 24, y: 0, width: 16, height: 64 });
  });

  it("квадрат занимает весь квадрат 64 точки", () => {
    expect(thumbnailRect([96, 96])).toEqual({ x: 0, y: 0, width: 64, height: 64 });
  });
});
