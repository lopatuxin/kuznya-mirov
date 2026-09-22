import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchText, loadGameOptions } from "./gameOptions";

type MockResponse = { ok: boolean; text: string };

function mockFetch(responses: Record<string, MockResponse>): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const response = responses[url];
      if (!response) throw new Error(`неожиданный запрос в тесте: ${url}`);
      return { ok: response.ok, text: async () => response.text } as Response;
    }),
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("loadGameOptions", () => {
  it("отдаёт все игры, если у каждой доступен и разобрался game.json", async () => {
    mockFetch({
      "/games/index.json": { ok: true, text: '["snake","arkanoid"]' },
      "/games/snake/game.json": { ok: true, text: JSON.stringify({ name: "Змейка" }) },
      "/games/arkanoid/game.json": { ok: true, text: JSON.stringify({ name: "Арканоид" }) },
    });

    const result = await loadGameOptions();

    expect(result.options).toEqual([
      { id: "snake", name: "Змейка" },
      { id: "arkanoid", name: "Арканоид" },
    ]);
    expect(result.failures).toEqual([]);
  });

  it("отдаёт исправные игры и отдельно причины сломанных, когда у одной игры недоступен game.json", async () => {
    mockFetch({
      "/games/index.json": { ok: true, text: '["snake","arkanoid"]' },
      "/games/snake/game.json": { ok: true, text: JSON.stringify({ name: "Змейка" }) },
      "/games/arkanoid/game.json": { ok: false, text: "" },
    });

    const result = await loadGameOptions();

    expect(result.options).toEqual([{ id: "snake", name: "Змейка" }]);
    expect(result.failures).toHaveLength(1);
    expect(result.failures[0]?.id).toBe("arkanoid");
    expect(result.failures[0]?.reason.length).toBeGreaterThan(0);
  });

  it("считает игру сломанной, а не роняет остальные, когда в её game.json нет поля name", async () => {
    mockFetch({
      "/games/index.json": { ok: true, text: '["snake","arkanoid"]' },
      "/games/snake/game.json": { ok: true, text: JSON.stringify({ name: "Змейка" }) },
      "/games/arkanoid/game.json": { ok: true, text: "{}" },
    });

    const result = await loadGameOptions();

    expect(result.options).toEqual([{ id: "snake", name: "Змейка" }]);
    expect(result.failures.map((failure) => failure.id)).toEqual(["arkanoid"]);
  });

  it("бросает исключение, только когда недоступен сам games/index.json", async () => {
    mockFetch({});

    await expect(loadGameOptions()).rejects.toThrow();
  });
});

describe("fetchText", () => {
  it("всегда сверяет файл игры со стендом, а не берёт копию из кэша браузера на догадку", async () => {
    const fetchMock = vi.fn(async () => ({ ok: true, text: async () => "{}" }) as Response);
    vi.stubGlobal("fetch", fetchMock);

    await fetchText("/games/tetris/screens.json");

    expect(fetchMock).toHaveBeenCalledWith("/games/tetris/screens.json", { cache: "no-cache" });
  });
});
