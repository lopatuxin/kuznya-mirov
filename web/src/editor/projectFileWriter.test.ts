import { afterEach, describe, expect, it, vi } from "vitest";
import { writeProjectFile } from "./projectFileWriter";
import type { ProjectSource } from "./projectSource";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("writeProjectFile — проект из списка", () => {
  const source: ProjectSource = { kind: "listed", name: "tetris" };

  it("шлёт PUT /games/<имя>/<путь> с текстом файла в теле", async () => {
    const fetchMock = vi.fn(async () => ({ ok: true, status: 204 }) as Response);
    vi.stubGlobal("fetch", fetchMock);

    const result = await writeProjectFile(source, "scene.json", '{"objects":[]}');

    expect(result).toEqual({ ok: true });
    expect(fetchMock).toHaveBeenCalledWith(
      "/games/tetris/scene.json",
      expect.objectContaining({ method: "PUT", body: '{"objects":[]}' }),
    );
  });

  it("ответ вне 2xx — причина с кодом ответа", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false, status: 403 }) as Response));

    const result = await writeProjectFile(source, "scene.json", "{}");

    expect(result).toEqual({ ok: false, reason: "сервер отказал в записи (код 403)" });
  });

  it("сетевая ошибка — причина текстом ошибки", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new Error("Failed to fetch");
      }),
    );

    const result = await writeProjectFile(source, "scene.json", "{}");

    expect(result).toEqual({ ok: false, reason: "Failed to fetch" });
  });
});

describe("writeProjectFile — папка с диска", () => {
  function createFolderSource(write: (text: string) => Promise<void>): ProjectSource {
    const fileHandle = {
      createWritable: vi.fn(async () => ({
        write: vi.fn(write),
        close: vi.fn(async () => {}),
      })),
    } as unknown as FileSystemFileHandle;
    const handle = {
      getFileHandle: vi.fn(async () => fileHandle),
      getDirectoryHandle: vi.fn(),
    } as unknown as FileSystemDirectoryHandle;
    return { kind: "folder", handle, displayName: "проект" };
  }

  it("пишет файл через createWritable дескриптора", async () => {
    const written: string[] = [];
    const source = createFolderSource(async (text) => {
      written.push(text);
    });

    const result = await writeProjectFile(source, "scene.json", '{"objects":[]}');

    expect(result).toEqual({ ok: true });
    expect(written).toEqual(['{"objects":[]}']);
  });

  it("отказ записи (например, отозванное разрешение) — причина текстом ошибки", async () => {
    const source = createFolderSource(async () => {
      throw new Error("NotAllowedError");
    });

    const result = await writeProjectFile(source, "scene.json", "{}");

    expect(result).toEqual({ ok: false, reason: "NotAllowedError" });
  });

  it("путь с .. — отказ без обращения к дескриптору", async () => {
    const handle = { getFileHandle: vi.fn(), getDirectoryHandle: vi.fn() } as unknown as FileSystemDirectoryHandle;
    const source: ProjectSource = { kind: "folder", handle, displayName: "проект" };

    const result = await writeProjectFile(source, "../secret.json", "{}");

    expect(result.ok).toBe(false);
    expect(handle.getFileHandle).not.toHaveBeenCalled();
  });
});
