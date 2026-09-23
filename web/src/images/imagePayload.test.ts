import { afterEach, describe, expect, it, vi } from "vitest";
import { buildImagePayload, fetchImageBytes, type ImageEntry } from "./imagePayload";

function stubWorkingBrowserDecoder(): void {
  const bitmap = { width: 1, height: 1, close: vi.fn() };
  vi.stubGlobal("createImageBitmap", vi.fn(async () => bitmap));
  vi.stubGlobal("document", {
    createElement: () => ({
      width: 0,
      height: 0,
      getContext: () => ({ drawImage: vi.fn(), getImageData: () => ({ data: new Uint8ClampedArray(4) }) }),
    }),
  } as unknown as Document);
}

describe("fetchImageBytes + buildImagePayload", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("номера и порядок записей для load совпадают со списком из read_texts, отсутствующий файл — приговор missing", async () => {
    stubWorkingBrowserDecoder();

    // Номера из `read_texts` — внутреннее дело движка и порядком с местом в таблице не связаны
    // («Картинки» → пункт 11), поэтому нарочно не по возрастанию.
    const images: ImageEntry[] = [
      { index: 5, name: "head", path: "images/head.png" },
      { index: 2, name: "tail", path: "images/tail.png" },
      { index: 9, name: "food", path: "images/food.png" },
    ];
    const bytesByPath = new Map<string, Uint8Array | null>([
      ["images/head.png", new Uint8Array([1])],
      ["images/tail.png", null],
      ["images/food.png", new Uint8Array([2])],
    ]);
    const readBinary = async (path: string): Promise<Uint8Array | null> => bytesByPath.get(path) ?? null;

    const loaded = await fetchImageBytes(images, readBinary);
    const payload = await buildImagePayload(loaded);

    expect(payload.map((entry) => entry.index)).toEqual([5, 2, 9]);
    expect(payload[1]).toEqual({ index: 2, verdict: "missing" });
    expect(payload[0].verdict).toBe("ok");
    expect(payload[2].verdict).toBe("ok");
  });
});
