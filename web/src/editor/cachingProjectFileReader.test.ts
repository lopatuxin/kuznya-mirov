import { describe, expect, it, vi } from "vitest";
import type { ProjectFileReader } from "../projectLoader";
import { createCachingProjectFileReader, createOverridingReader } from "./cachingProjectFileReader";

function fakeBaseReader(files: Record<string, string>): ProjectFileReader {
  return {
    readText: vi.fn(async (path: string) => files[path] ?? null),
    readBinary: vi.fn(async () => null),
  };
}

describe("createCachingProjectFileReader", () => {
  it("reader запоминает прочитанный текст", async () => {
    const base = fakeBaseReader({ "rules.json": "{}" });
    const cache = createCachingProjectFileReader(base);

    expect(cache.getCachedText("rules.json")).toBeUndefined();
    await cache.reader.readText("rules.json");
    expect(cache.getCachedText("rules.json")).toBe("{}");
  });

  it("cachedReader не ходит к базовой читалке", async () => {
    const base = fakeBaseReader({ "rules.json": "{}" });
    const cache = createCachingProjectFileReader(base);
    await cache.reader.readText("rules.json");

    const result = await cache.cachedReader.readText("rules.json");

    expect(result).toBe("{}");
    expect(base.readText).toHaveBeenCalledTimes(1);
  });

  it("непрочитанный путь у cachedReader — null, без обращения к базовой читалке", async () => {
    const base = fakeBaseReader({});
    const cache = createCachingProjectFileReader(base);

    expect(await cache.cachedReader.readText("scene.json")).toBe(null);
    expect(base.readText).not.toHaveBeenCalled();
  });

  it("setCachedText делает свою запись новой правдой диска", async () => {
    const base = fakeBaseReader({});
    const cache = createCachingProjectFileReader(base);

    cache.setCachedText("scene.json", '{"objects":[]}');

    expect(cache.getCachedText("scene.json")).toBe('{"objects":[]}');
  });

  it("getWriteCount растёт на каждой записи — требование 24, метка перезагрузки", async () => {
    const base = fakeBaseReader({});
    const cache = createCachingProjectFileReader(base);

    expect(cache.getWriteCount()).toBe(0);
    cache.setCachedText("scene.json", '{"objects":[]}');
    expect(cache.getWriteCount()).toBe(1);
    cache.setCachedText("properties.json", "{}");
    expect(cache.getWriteCount()).toBe(2);
  });
});

describe("createOverridingReader", () => {
  it("отдаёт текст правки для своего пути, остальное — из кэша", async () => {
    const base = fakeBaseReader({ "rules.json": "{}" });
    const cache = createCachingProjectFileReader(base);
    await cache.reader.readText("rules.json");

    const reader = createOverridingReader(cache.cachedReader, { "scene.json": '{"objects":[]}' });

    expect(await reader.readText("scene.json")).toBe('{"objects":[]}');
    expect(await reader.readText("rules.json")).toBe("{}");
    expect(base.readText).toHaveBeenCalledTimes(1);
  });
});
