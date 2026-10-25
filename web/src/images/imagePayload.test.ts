import { afterEach, describe, expect, it, vi } from "vitest";
import { FakeVideo } from "./fakeVideo";
import { buildImagePayload, buildVideoPayload, fetchImageBytes, isVideoPath, type ImageEntry } from "./imagePayload";

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

describe("видео", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("видео — запись с путём на .mp4 в любом регистре, остальное картинка", () => {
    expect(isVideoPath("images/grass.mp4")).toBe(true);
    expect(isVideoPath("images/GRASS.MP4")).toBe(true);
    expect(isVideoPath("images/grass.png")).toBe(false);
    expect(isVideoPath("images/mp4.png")).toBe(false);
  });

  it("buildVideoPayload по номерам записей: файла нет — missing без проигрывателя", async () => {
    const payload = await buildVideoPayload([{ index: 4, path: "images/grass.mp4", bytes: null }]);

    expect(payload).toEqual([{ index: 4, verdict: "missing" }]);
  });

  it("fetchImageBytes читает обратный файл по reverse записи; без reverse поля reverseBytes нет, ненайденный обратный — null", async () => {
    const bytesByPath = new Map<string, Uint8Array>([
      ["images/a.mp4", new Uint8Array([1])],
      ["images/a-reverse.mp4", new Uint8Array([2])],
      ["images/b.mp4", new Uint8Array([3])],
    ]);
    const readBinary = async (path: string): Promise<Uint8Array | null> => bytesByPath.get(path) ?? null;

    const loaded = await fetchImageBytes(
      [
        { index: 0, path: "images/a.mp4", reverse: "images/a-reverse.mp4" },
        { index: 1, path: "images/b.mp4", reverse: "images/b-reverse.mp4" },
        { index: 2, path: "images/c.png" },
      ],
      readBinary,
    );

    expect(loaded[0]?.reverseBytes).toEqual(new Uint8Array([2]));
    expect(loaded[1]?.reverseBytes).toBeNull();
    expect(loaded[2]).not.toHaveProperty("reverseBytes");
  });

  it("buildVideoPayload отдаёт обратные байты разбору видео: ответ несёт reverse", async () => {
    const main = new FakeVideo("loadeddata", 4, 4);
    const reverse = new FakeVideo("loadeddata", 4, 4);
    const players = [main, reverse];
    let made = 0;
    vi.stubGlobal("document", {
      createElement: (tag: string) =>
        tag === "video"
          ? players[made++]
          : { width: 0, height: 0, getContext: () => ({ drawImage: vi.fn(), getImageData: (_x: number, _y: number, width: number, height: number) => ({ data: new Uint8ClampedArray(width * height * 4) }) }) },
    } as unknown as Document);
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:video");

    const payload = await buildVideoPayload([{ index: 4, path: "images/a.mp4", bytes: new Uint8Array([1]), reverseBytes: new Uint8Array([2]) }]);

    expect(payload[0]).toMatchObject({ index: 4, verdict: "ok", player: main, reverse: { verdict: "ok", player: reverse } });
  });

  it("buildVideoPayload: сбой чтения кадра одного видео освобождает проигрыватели всех и уходит дальше", async () => {
    const players = [new FakeVideo("loadeddata", 4, 4), new FakeVideo("loadeddata", 4, 4)];
    let made = 0;
    let read = 0;
    const getImageData = (_x: number, _y: number, width: number, height: number) => {
      read += 1;
      if (read === 2) throw new Error("не хватило памяти");
      return { data: new Uint8ClampedArray(width * height * 4) };
    };
    vi.stubGlobal("document", {
      createElement: (tag: string) => (tag === "video" ? players[made++] : { width: 0, height: 0, getContext: () => ({ drawImage: vi.fn(), getImageData }) }),
    } as unknown as Document);
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:video");
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});

    const loading = buildVideoPayload([
      { index: 1, path: "images/grass.mp4", bytes: new Uint8Array([1]) },
      { index: 2, path: "images/panicles.mp4", bytes: new Uint8Array([2]) },
    ]);

    await expect(loading).rejects.toThrow("не хватило памяти");
    for (const player of players) expect(player.removeAttribute).toHaveBeenCalledWith("src");
  });
});
