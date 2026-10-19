import { afterEach, describe, expect, it, vi } from "vitest";
import { buildModelPayload, readModelImageBytes } from "./modelImages";

function chunk(type: number, data: Uint8Array, pad: number): Uint8Array {
  const length = Math.ceil(data.length / 4) * 4;
  const out = new Uint8Array(8 + length).fill(pad);
  const view = new DataView(out.buffer);
  view.setUint32(0, length, true);
  view.setUint32(4, type, true);
  out.set(data, 8);
  return out;
}

/** Файл `.glb` из JSON-куска и, если дан, BIN-куска — как его кладёт экспорт Blender. */
function glb(json: object, bin: Uint8Array | null): Uint8Array {
  const chunks = [chunk(0x4e4f534a, new TextEncoder().encode(JSON.stringify(json)), 0x20), ...(bin === null ? [] : [chunk(0x004e4942, bin, 0)])];
  const total = 12 + chunks.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(total);
  const view = new DataView(out.buffer);
  view.setUint32(0, 0x46546c67, true);
  view.setUint32(4, 2, true);
  view.setUint32(8, total, true);
  let offset = 12;
  for (const part of chunks) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

const BIN = Uint8Array.from({ length: 32 }, (_, index) => index);

describe("readModelImageBytes", () => {
  it("PNG и JPEG берёт из буфера 0 по bufferView, со смещением и длиной вида, по номеру картинки в файле", () => {
    const file = glb(
      {
        bufferViews: [
          { buffer: 0, byteOffset: 4, byteLength: 6 },
          { buffer: 0, byteLength: 3 },
        ],
        images: [
          { bufferView: 0, mimeType: "image/png" },
          { bufferView: 1, mimeType: "image/jpeg" },
        ],
      },
      BIN,
    );

    const images = readModelImageBytes(file);

    expect(images.map((bytes) => (bytes === null ? null : Array.from(bytes)))).toEqual([[4, 5, 6, 7, 8, 9], [0, 1, 2]]);
  });

  it("внешний uri, другой тип, картинка без вида, чужой буфер и вид за пределами буфера дают null на своём месте", () => {
    const file = glb(
      {
        bufferViews: [
          { buffer: 0, byteOffset: 0, byteLength: 4 },
          { buffer: 1, byteOffset: 0, byteLength: 4 },
          { buffer: 0, byteOffset: 30, byteLength: 8 },
          { buffer: 0, byteOffset: 0, byteLength: 0 },
        ],
        images: [
          { uri: "base.png" },
          { bufferView: 0, mimeType: "image/webp" },
          { mimeType: "image/png" },
          { bufferView: 1, mimeType: "image/png" },
          { bufferView: 2, mimeType: "image/png" },
          { bufferView: 3, mimeType: "image/png" },
          { bufferView: 9, mimeType: "image/png" },
          { bufferView: 0, mimeType: "image/png" },
        ],
      },
      BIN,
    );

    const images = readModelImageBytes(file);

    expect(images).toHaveLength(8);
    expect(images.slice(0, 7)).toEqual(new Array(7).fill(null));
    expect(Array.from(images[7] as Uint8Array)).toEqual([0, 1, 2, 3]);
  });

  it("файл без картинок, без BIN-куска и не glTF картинок не даёт, не падая", () => {
    expect(readModelImageBytes(glb({ meshes: [] }, BIN))).toEqual([]);
    expect(readModelImageBytes(glb({ bufferViews: [{ buffer: 0, byteLength: 4 }], images: [{ bufferView: 0, mimeType: "image/png" }] }, null))).toEqual([null]);
    expect(readModelImageBytes(new Uint8Array([1, 2, 3]))).toEqual([]);
    expect(readModelImageBytes(new TextEncoder().encode("не glTF, а просто текст длиннее заголовка"))).toEqual([]);
  });

  it("читает файл, лежащий в середине чужого буфера: смещение байтов вида не сбивается", () => {
    const file = glb({ bufferViews: [{ buffer: 0, byteOffset: 8, byteLength: 2 }], images: [{ bufferView: 0, mimeType: "image/png" }] }, BIN);
    const shifted = new Uint8Array(file.length + 5);
    shifted.set(file, 5);

    const [image] = readModelImageBytes(shifted.subarray(5));

    expect(Array.from(image as Uint8Array)).toEqual([8, 9]);
  });
});

describe("buildModelPayload", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function stubBrowserDecoder(width: number, height: number): ReturnType<typeof vi.fn> {
    const createImageBitmap = vi.fn(async () => ({ width, height, close: vi.fn() }));
    vi.stubGlobal("createImageBitmap", createImageBitmap);
    vi.stubGlobal("document", {
      createElement: () => ({
        width: 0,
        height: 0,
        getContext: () => ({ drawImage: vi.fn(), getImageData: () => ({ data: new Uint8ClampedArray(width * height * 4) }) }),
      }),
    } as unknown as Document);
    return createImageBitmap;
  }

  it("номер модели и файл уходят как есть, у картинок номер — место в images файла, не в списке разжатых", async () => {
    const createImageBitmap = stubBrowserDecoder(2, 1);
    const file = glb(
      {
        bufferViews: [{ buffer: 0, byteOffset: 0, byteLength: 4 }],
        images: [{ uri: "outer.png" }, { bufferView: 0, mimeType: "image/png" }],
      },
      BIN,
    );

    const payload = await buildModelPayload([{ index: 7, path: "models/house.glb", bytes: file }]);

    expect(payload).toEqual([
      {
        index: 7,
        bytes: file,
        images: [
          { index: 0, verdict: "rejected" },
          { index: 1, verdict: "ok", width: 2, height: 1, pixels: expect.any(Uint8Array) },
        ],
      },
    ]);
    // Разжимается только годная картинка, и ровно её байты.
    expect(createImageBitmap).toHaveBeenCalledTimes(1);
    const blob = (createImageBitmap.mock.calls[0] as unknown as [Blob])[0];
    expect(blob.size).toBe(4);
  });

  it("браузер не разжал картинку — rejected; файла нет — bytes null без картинок", async () => {
    vi.stubGlobal(
      "createImageBitmap",
      vi.fn(async () => {
        throw new Error("не PNG");
      }),
    );
    const file = glb({ bufferViews: [{ buffer: 0, byteLength: 4 }], images: [{ bufferView: 0, mimeType: "image/png" }] }, BIN);

    const payload = await buildModelPayload([
      { index: 0, path: "models/a.glb", bytes: file },
      { index: 1, path: "models/missing.glb", bytes: null },
    ]);

    expect(payload[0]?.images).toEqual([{ index: 0, verdict: "rejected" }]);
    expect(payload[1]).toEqual({ index: 1, bytes: null, images: [] });
  });
});
