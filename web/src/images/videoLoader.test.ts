import { afterEach, describe, expect, it, vi } from "vitest";
import { FakeVideo } from "./fakeVideo";
import {
  composeFirstFrame,
  createVideoPlayerKeeper,
  decodeReverseVideo,
  decodeVideo,
  FIRST_FRAME_WAIT_MS,
  releaseDecodedVideos,
  releaseVideoPlayer,
} from "./videoLoader";

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

/** Проигрыватели выдаются по порядку заведения: сначала обычный файл, потом обратный. */
function stubBrowserWithPlayers(videos: FakeVideo[], getImageData: () => { data: Uint8ClampedArray } = () => ({ data: FILE_RGBA })): void {
  const context = { drawImage: vi.fn(), getImageData };
  let made = 0;
  vi.stubGlobal("document", {
    createElement: (tag: string) => (tag === "video" ? videos[made++] : { width: 0, height: 0, getContext: () => context }),
  } as unknown as Document);
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:video-1");
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
}

describe("decodeReverseVideo", () => {
  it("файла нет — missing, проигрыватель не заводится", async () => {
    const createElement = vi.fn();
    vi.stubGlobal("document", { createElement } as unknown as Document);

    expect(await decodeReverseVideo(null)).toEqual({ verdict: "missing" });
    expect(createElement).not.toHaveBeenCalled();
  });

  it("первый кадр получен — ok с таким же проигрывателем, как у основного видео", async () => {
    const video = new FakeVideo("loadeddata");
    stubBrowser(video);

    const result = await decodeReverseVideo(new Uint8Array([1]));

    expect(result).toEqual({ verdict: "ok", player: video });
    expect([video.muted, video.playsInline, video.preload]).toEqual([true, true, "auto"]);
    expect(video.src).toBe("blob:video-1");
  });

  it("браузер ответил ошибкой — rejected, проигрыватель освобождён", async () => {
    const video = new FakeVideo("error");
    stubBrowser(video);

    expect(await decodeReverseVideo(new Uint8Array([1]))).toEqual({ verdict: "rejected" });
    expect(video.removeAttribute).toHaveBeenCalledWith("src");
  });
});

describe("decodeVideo с reverse", () => {
  it("без обратных байтов ответ без поля reverse", async () => {
    stubBrowser(new FakeVideo("loadeddata"));

    expect(await decodeVideo(new Uint8Array([1]))).not.toHaveProperty("reverse");
  });

  it("оба файла разобраны — ok с проигрывателем обратного файла в reverse", async () => {
    const main = new FakeVideo("loadeddata", 2, 4);
    const reverse = new FakeVideo("loadeddata", 2, 4);
    stubBrowserWithPlayers([main, reverse]);

    const result = await decodeVideo(new Uint8Array([1]), new Uint8Array([2]));

    expect(result).toMatchObject({ verdict: "ok", player: main, reverse: { verdict: "ok", player: reverse } });
  });

  it("обратного файла нет — основной ok, reverse говорит missing", async () => {
    const main = new FakeVideo("loadeddata");
    stubBrowserWithPlayers([main]);

    const result = await decodeVideo(new Uint8Array([1]), null);

    expect(result).toMatchObject({ verdict: "ok", player: main, reverse: { verdict: "missing" } });
  });

  it("обратный файл браузер не взял — reverse rejected, основной играет", async () => {
    const main = new FakeVideo("loadeddata");
    const reverse = new FakeVideo("error");
    stubBrowserWithPlayers([main, reverse]);

    const result = await decodeVideo(new Uint8Array([1]), new Uint8Array([2]));

    expect(result).toMatchObject({ verdict: "ok", reverse: { verdict: "rejected" } });
    expect(main.pause).not.toHaveBeenCalled();
  });

  it("основной файл не взят — освобождён и проигрыватель обратного", async () => {
    const main = new FakeVideo("error");
    const reverse = new FakeVideo("loadeddata");
    stubBrowserWithPlayers([main, reverse]);

    expect(await decodeVideo(new Uint8Array([1]), new Uint8Array([2]))).toEqual({ verdict: "rejected" });
    expect(main.removeAttribute).toHaveBeenCalledWith("src");
    expect(reverse.removeAttribute).toHaveBeenCalledWith("src");
  });

  it("сбой чтения кадра основного файла освобождает и обратный проигрыватель и уходит дальше", async () => {
    const main = new FakeVideo("loadeddata");
    const reverse = new FakeVideo("loadeddata");
    stubBrowserWithPlayers([main, reverse], () => {
      throw new Error("не хватило памяти");
    });

    await expect(decodeVideo(new Uint8Array([1]), new Uint8Array([2]))).rejects.toThrow("не хватило памяти");
    expect(main.removeAttribute).toHaveBeenCalledWith("src");
    expect(reverse.removeAttribute).toHaveBeenCalledWith("src");
  });
});

describe("releaseDecodedVideos", () => {
  it("освобождает и обычные проигрыватели, и обратные; missing и rejected пропускает", () => {
    stubBrowser(new FakeVideo("loadeddata"));
    const main = new FakeVideo("loadeddata");
    const reverse = new FakeVideo("loadeddata");
    const frame = { width: 1, height: 1, pixels: new Uint8Array(4) };

    releaseDecodedVideos([
      { verdict: "ok", width: 1, height: 2, player: main as unknown as HTMLVideoElement, frame, reverse: { verdict: "ok", player: reverse as unknown as HTMLVideoElement } },
      { verdict: "missing" },
      { verdict: "rejected" },
    ]);

    expect(main.pause).toHaveBeenCalledTimes(1);
    expect(reverse.pause).toHaveBeenCalledTimes(1);
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
