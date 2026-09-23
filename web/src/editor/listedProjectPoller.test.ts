import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pollListedProject, type FileFingerprint } from "./listedProjectPoller";

type MockResponse = { status: number; headers?: Record<string, string> };

function mockFetch(responsesByUrl: Record<string, MockResponse>): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn(async (url: string) => {
    const response = responsesByUrl[url];
    if (response === undefined) throw new Error(`неожиданный запрос в тесте: ${url}`);
    return {
      status: response.status,
      headers: { get: (name: string) => response.headers?.[name] ?? null },
    } as unknown as Response;
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("pollListedProject", () => {
  it("опрашивает ровно пути последней загрузки", async () => {
    const fetchMock = mockFetch({
      "/games/tetris/game.json": { status: 200, headers: { ETag: "a" } },
      "/games/tetris/scene.json": { status: 200, headers: { ETag: "b" } },
    });

    const dispose = pollListedProject("/games/tetris/", ["game.json", "scene.json"], new Map(), () => {});
    await vi.advanceTimersByTimeAsync(0);

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock).toHaveBeenCalledWith("/games/tetris/game.json", { method: "HEAD", cache: "no-store" });
    expect(fetchMock).toHaveBeenCalledWith("/games/tetris/scene.json", { method: "HEAD", cache: "no-store" });

    dispose();
  });

  it("первый опрос по пути, которого раньше не было в таблице, только запоминает отпечаток", async () => {
    mockFetch({ "/games/tetris/game.json": { status: 200, headers: { ETag: "a" } } });
    const onChanged = vi.fn();

    const dispose = pollListedProject("/games/tetris/", ["game.json"], new Map(), onChanged);
    await vi.advanceTimersByTimeAsync(0);

    expect(onChanged).not.toHaveBeenCalled();
    dispose();
  });

  it("отпечаток пути переживает пересоздание опроса и ловит правку до первого тика нового опроса", async () => {
    let etag = "a";
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ status: 200, headers: { get: (name: string) => (name === "ETag" ? etag : null) } }) as unknown as Response),
    );
    // Общая таблица «путь → отпечаток» на весь открытый проект («Редактор», требование 32) —
    // передаётся заново при каждом перезапуске опроса, как это делает useProjectEngine после
    // каждой загрузки.
    const fingerprints = new Map<string, FileFingerprint>();

    const firstPoll = pollListedProject("/games/tetris/", ["game.json"], fingerprints, () => {});
    await vi.advanceTimersByTimeAsync(0);
    firstPoll();

    // Правка происходит после того, как загрузку прочитала файл (закрыв прежний опрос), но до
    // первого тика опроса, перезапущенного под новый список путей.
    etag = "b";
    const onChanged = vi.fn();
    const secondPoll = pollListedProject("/games/tetris/", ["game.json"], fingerprints, onChanged);
    await vi.advanceTimersByTimeAsync(0);

    expect(onChanged).toHaveBeenCalledTimes(1);
    secondPoll();
  });

  it("одинаковые ответы не дают onChanged", async () => {
    mockFetch({ "/games/tetris/game.json": { status: 200, headers: { ETag: "a", "Last-Modified": "d", "Content-Length": "2" } } });
    const onChanged = vi.fn();

    const dispose = pollListedProject("/games/tetris/", ["game.json"], new Map(), onChanged);
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(1000);
    await vi.advanceTimersByTimeAsync(1000);

    expect(onChanged).not.toHaveBeenCalled();
    dispose();
  });

  it("изменение ETag даёт onChanged", async () => {
    let etag = "a";
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ status: 200, headers: { get: (name: string) => (name === "ETag" ? etag : null) } }) as unknown as Response),
    );
    const onChanged = vi.fn();

    const dispose = pollListedProject("/games/tetris/", ["game.json"], new Map(), onChanged);
    await vi.advanceTimersByTimeAsync(0);
    etag = "b";
    await vi.advanceTimersByTimeAsync(1000);

    expect(onChanged).toHaveBeenCalledTimes(1);
    dispose();
  });

  it("изменение Last-Modified даёт onChanged", async () => {
    let lastModified = "mon";
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          ({ status: 200, headers: { get: (name: string) => (name === "Last-Modified" ? lastModified : null) } }) as unknown as Response,
      ),
    );
    const onChanged = vi.fn();

    const dispose = pollListedProject("/games/tetris/", ["game.json"], new Map(), onChanged);
    await vi.advanceTimersByTimeAsync(0);
    lastModified = "tue";
    await vi.advanceTimersByTimeAsync(1000);

    expect(onChanged).toHaveBeenCalledTimes(1);
    dispose();
  });

  it("изменение Content-Length даёт onChanged", async () => {
    let contentLength = "10";
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          ({ status: 200, headers: { get: (name: string) => (name === "Content-Length" ? contentLength : null) } }) as unknown as Response,
      ),
    );
    const onChanged = vi.fn();

    const dispose = pollListedProject("/games/tetris/", ["game.json"], new Map(), onChanged);
    await vi.advanceTimersByTimeAsync(0);
    contentLength = "20";
    await vi.advanceTimersByTimeAsync(1000);

    expect(onChanged).toHaveBeenCalledTimes(1);
    dispose();
  });

  it("появление файла (404 → 200) даёт onChanged", async () => {
    let status = 404;
    vi.stubGlobal("fetch", vi.fn(async () => ({ status, headers: { get: () => null } }) as unknown as Response));
    const onChanged = vi.fn();

    const dispose = pollListedProject("/games/tetris/", ["images/head.png"], new Map(), onChanged);
    await vi.advanceTimersByTimeAsync(0);
    status = 200;
    await vi.advanceTimersByTimeAsync(1000);

    expect(onChanged).toHaveBeenCalledTimes(1);
    dispose();
  });

  it("пропажа файла (200 → 404) даёт onChanged", async () => {
    let status = 200;
    vi.stubGlobal("fetch", vi.fn(async () => ({ status, headers: { get: () => null } }) as unknown as Response));
    const onChanged = vi.fn();

    const dispose = pollListedProject("/games/tetris/", ["images/head.png"], new Map(), onChanged);
    await vi.advanceTimersByTimeAsync(0);
    status = 404;
    await vi.advanceTimersByTimeAsync(1000);

    expect(onChanged).toHaveBeenCalledTimes(1);
    dispose();
  });

  it("отказ сети даёт onChanged", async () => {
    let shouldFail = false;
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        if (shouldFail) throw new Error("сеть недоступна");
        return { status: 200, headers: { get: () => null } } as unknown as Response;
      }),
    );
    const onChanged = vi.fn();

    const dispose = pollListedProject("/games/tetris/", ["game.json"], new Map(), onChanged);
    await vi.advanceTimersByTimeAsync(0);
    shouldFail = true;
    await vi.advanceTimersByTimeAsync(1000);

    expect(onChanged).toHaveBeenCalledTimes(1);
    dispose();
  });

  it("после остановки запросов нет", async () => {
    const fetchMock = mockFetch({ "/games/tetris/game.json": { status: 200, headers: { ETag: "a" } } });

    const dispose = pollListedProject("/games/tetris/", ["game.json"], new Map(), () => {});
    await vi.advanceTimersByTimeAsync(0);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    dispose();
    await vi.advanceTimersByTimeAsync(5000);

    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("при зависшем ответе новые запросы не уходят", async () => {
    const pendingResolve: { current: (() => void) | null } = { current: null };
    const fetchMock = vi.fn(
      () =>
        new Promise<Response>((resolve) => {
          pendingResolve.current = () => resolve({ status: 200, headers: { get: () => null } } as unknown as Response);
        }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const dispose = pollListedProject("/games/tetris/", ["game.json"], new Map(), () => {});
    await vi.advanceTimersByTimeAsync(0);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    // Следующий тик планировался бы через 1с у `setInterval`, вне зависимости от того, ответил ли
    // сервер на предыдущий запрос — здесь второй запрос не должен уйти, пока первый висит.
    await vi.advanceTimersByTimeAsync(5000);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    pendingResolve.current?.();
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(1000);
    expect(fetchMock).toHaveBeenCalledTimes(2);

    dispose();
  });
});
