import { afterEach, describe, expect, it, vi } from "vitest";
import { decodeImage } from "./imageLoader";

function makeFakeCanvas(pixels: Uint8ClampedArray) {
  const context = {
    drawImage: vi.fn(),
    getImageData: vi.fn(() => ({ data: pixels })),
  };
  return { width: 0, height: 0, getContext: () => context };
}

describe("decodeImage", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("файла нет — приговор missing без обращения к браузерному декодеру", async () => {
    const createImageBitmap = vi.fn();
    vi.stubGlobal("createImageBitmap", createImageBitmap);

    expect(await decodeImage(null)).toEqual({ verdict: "missing" });
    expect(createImageBitmap).not.toHaveBeenCalled();
  });

  it("браузер не смог разжать файл — приговор rejected, а не сбой движка", async () => {
    vi.stubGlobal(
      "createImageBitmap",
      vi.fn(async () => {
        throw new Error("не PNG");
      }),
    );

    expect(await decodeImage(new Uint8Array([1, 2, 3]))).toEqual({ verdict: "rejected" });
  });

  it("удачное разжатие — приговор ok, размер из bitmap и точки длиной width * height * 4", async () => {
    const width = 2;
    const height = 3;
    const pixels = new Uint8ClampedArray(width * height * 4).map((_, i) => i);
    const bitmap = { width, height, close: vi.fn() };
    const createImageBitmap = vi.fn(async () => bitmap);
    vi.stubGlobal("createImageBitmap", createImageBitmap);
    vi.stubGlobal("document", { createElement: () => makeFakeCanvas(pixels) } as unknown as Document);

    const result = await decodeImage(new Uint8Array([1, 2, 3]));

    expect(result.verdict).toBe("ok");
    if (result.verdict !== "ok") throw new Error("unreachable");
    expect(result.width).toBe(width);
    expect(result.height).toBe(height);
    expect(result.pixels).toHaveLength(width * height * 4);
    expect(bitmap.close).toHaveBeenCalled();
    // Без этих двух ключей точки приходят домноженными на прозрачность и пересчитанными под
    // монитор — полупрозрачные края картинок поедут по цвету незаметно для остальных тестов.
    expect(createImageBitmap).toHaveBeenCalledWith(expect.any(Blob), {
      premultiplyAlpha: "none",
      colorSpaceConversion: "none",
    });
  });

  it("у канваса нет 2D-контекста — приговор rejected", async () => {
    const bitmap = { width: 1, height: 1, close: vi.fn() };
    vi.stubGlobal("createImageBitmap", vi.fn(async () => bitmap));
    vi.stubGlobal(
      "document",
      { createElement: () => ({ width: 0, height: 0, getContext: () => null }) } as unknown as Document,
    );

    expect(await decodeImage(new Uint8Array([1]))).toEqual({ verdict: "rejected" });
    expect(bitmap.close).toHaveBeenCalled();
  });
});
