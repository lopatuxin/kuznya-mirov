import { afterEach, describe, expect, it, vi } from "vitest";
import { FakeVideo } from "./fakeVideo";
import { composeFirstFrame, createVideoPlayerKeeper, decodeVideo, FIRST_FRAME_WAIT_MS, releaseVideoPlayer } from "./videoLoader";

/** Файл 2 × 4: верхняя половина — четыре точки цвета, нижняя — четыре точки маски: белая, серая, чёрная, красная. */
const FILE_RGBA = new Uint8ClampedArray([
  10, 20, 30, 255, 40, 50, 60, 255,
  70, 80, 90, 255, 100, 110, 120, 255,
  255, 255, 255, 255, 100, 100, 100, 255,
  0, 0, 0, 255, 255, 0, 0, 255,
]);

function stubBrowser(video: FakeVideo): void {
  const context = { drawImage: vi.fn(), getImageData: vi.fn(() => ({ data: FILE_RGBA })) };
  vi.stubGlobal("document", {
    createElement: (tag: string) => (tag === "video" ? video : { width: 0, height: 0, getContext: () => context }),
  } as unknown as Document);
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:video-1");
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("composeFirstFrame", () => {
  it("высота кадра — половина файла, цвет из верхней половины, прозрачность — яркость 0,299R + 0,587G + 0,114B нижней", () => {
    const frame = composeFirstFrame(FILE_RGBA, 2, 4);

    expect([frame.width, frame.height]).toEqual([2, 2]);
    // Красная точка маски (255, 0, 0) даёт 0,299 · 255 ≈ 76: цвет соседней половины в маску не просачивается.
    expect([...frame.pixels]).toEqual([10, 20, 30, 255, 40, 50, 60, 100, 70, 80, 90, 0, 100, 110, 120, 76]);
  });
});

describe("decodeVideo", () => {
  it("файла нет — приговор missing, проигрыватель не заводится", async () => {
    const createElement = vi.fn();
    vi.stubGlobal("document", { createElement } as unknown as Document);

    expect(await decodeVideo(null)).toEqual({ verdict: "missing" });
    expect(createElement).not.toHaveBeenCalled();
  });

  it("первый кадр получен — ok с размером файла и проигрывателем без звука, по кругу, на Blob-адресе", async () => {
    const video = new FakeVideo("loadeddata", 2, 4);
    stubBrowser(video);

    const result = await decodeVideo(new Uint8Array([1, 2, 3]));

    expect(result.verdict).toBe("ok");
    if (result.verdict !== "ok") throw new Error("unreachable");
    expect([result.width, result.height]).toEqual([2, 4]);
    expect(result.player).toBe(video);
    expect([video.muted, video.loop, video.playsInline]).toEqual([true, true, true]);
    expect(video.src).toBe("blob:video-1");
    expect([result.frame.width, result.frame.height]).toEqual([2, 2]);
  });

  it("браузер ответил ошибкой — rejected, проигрыватель освобождён", async () => {
    const video = new FakeVideo("error");
    stubBrowser(video);

    expect(await decodeVideo(new Uint8Array([1]))).toEqual({ verdict: "rejected" });
    expect(video.pause).toHaveBeenCalled();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:video-1");
  });

  it("браузер не отдал ни кадра, ни ошибки за отведённое время — rejected, проигрыватель освобождён", async () => {
    vi.useFakeTimers();
    try {
      const video = new FakeVideo("silence");
      stubBrowser(video);

      const decoded = decodeVideo(new Uint8Array([1]));
      await vi.advanceTimersByTimeAsync(FIRST_FRAME_WAIT_MS);

      expect(await decoded).toEqual({ verdict: "rejected" });
      expect(video.pause).toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("у видео нет размера кадра — rejected", async () => {
    stubBrowser(new FakeVideo("loadeddata", 0, 0));

    expect(await decodeVideo(new Uint8Array([1]))).toEqual({ verdict: "rejected" });
  });
});

describe("releaseVideoPlayer и createVideoPlayerKeeper", () => {
  it("освобождение — пауза, снятый источник, отозванный Blob-адрес", () => {
    const video = new FakeVideo("loadeddata");
    stubBrowser(video);
    video.src = "blob:video-1";

    releaseVideoPlayer(video as unknown as HTMLVideoElement);

    expect(video.pause).toHaveBeenCalled();
    expect(video.removeAttribute).toHaveBeenCalledWith("src");
    expect(video.load).toHaveBeenCalled();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:video-1");
  });

  it("replace отпускает прежние проигрыватели, но не новые; releaseAll — все оставшиеся", () => {
    stubBrowser(new FakeVideo("loadeddata"));
    const first = new FakeVideo("loadeddata");
    const second = new FakeVideo("loadeddata");
    const keeper = createVideoPlayerKeeper();

    keeper.replace([first as unknown as HTMLVideoElement]);
    keeper.replace([second as unknown as HTMLVideoElement]);
    expect(first.pause).toHaveBeenCalledTimes(1);
    expect(second.pause).not.toHaveBeenCalled();

    keeper.releaseAll();
    expect(second.pause).toHaveBeenCalledTimes(1);
    expect(first.pause).toHaveBeenCalledTimes(1);
  });
});
