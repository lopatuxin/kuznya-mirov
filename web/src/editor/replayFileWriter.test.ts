import { afterEach, describe, expect, it, vi } from "vitest";
import { saveRecordingToProject } from "./replayFileWriter";
import type { ProjectSource } from "./projectSource";

const STARTED_AT = new Date(2026, 2, 4, 9, 5, 7);
const RECORDING_TEXT = '{"format":1,"steps":3,"events":[]}';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("saveRecordingToProject — проект из списка", () => {
  const source: ProjectSource = { kind: "listed", name: "tetris" };

  it("свободное имя — PUT в replays/<время начала>.json", async () => {
    const fetchMock = vi.fn(async (_url: string, init?: RequestInit) => {
      if (init?.method === "PUT") return { ok: true, status: 204 } as Response;
      return { ok: false, status: 404 } as Response; // HEAD — файла ещё нет
    });
    vi.stubGlobal("fetch", fetchMock);

    const result = await saveRecordingToProject(source, STARTED_AT, RECORDING_TEXT);

    expect(result).toEqual({ ok: true, fileName: "2026-03-04-09-05-07.json" });
    expect(fetchMock).toHaveBeenCalledWith("/games/tetris/replays/2026-03-04-09-05-07.json", expect.objectContaining({ method: "PUT", body: RECORDING_TEXT }));
  });

  it("имя занято — сохраняет с суффиксом -2 (требование 34)", async () => {
    const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
      if (init?.method === "PUT") return { ok: true, status: 204 } as Response;
      return { ok: url.toString().endsWith("2026-03-04-09-05-07.json") } as Response; // первое имя занято
    });
    vi.stubGlobal("fetch", fetchMock);

    const result = await saveRecordingToProject(source, STARTED_AT, RECORDING_TEXT);

    expect(result).toEqual({ ok: true, fileName: "2026-03-04-09-05-07-2.json" });
  });

  it("отказ записи — причина текстом", async () => {
    const fetchMock = vi.fn(async (_url: string, init?: RequestInit) => {
      if (init?.method === "PUT") return { ok: false, status: 500 } as Response;
      return { ok: false } as Response;
    });
    vi.stubGlobal("fetch", fetchMock);

    const result = await saveRecordingToProject(source, STARTED_AT, RECORDING_TEXT);

    expect(result).toEqual({ ok: false, reason: "сервер отказал в записи (код 500)" });
  });
});

describe("saveRecordingToProject — папка с диска", () => {
  function createFolderSource(existingFiles: Set<string>, write: (name: string, text: string) => Promise<void>): ProjectSource {
    function makeFileHandle(name: string): FileSystemFileHandle {
      return {
        createWritable: vi.fn(async () => ({
          write: vi.fn(async (text: string) => write(name, text)),
          close: vi.fn(async () => {}),
        })),
      } as unknown as FileSystemFileHandle;
    }
    const replaysDir = {
      getFileHandle: vi.fn(async (name: string, options?: { create?: boolean }) => {
        if (!existingFiles.has(name) && !options?.create) throw new DOMException("нет файла", "NotFoundError");
        return makeFileHandle(name);
      }),
    } as unknown as FileSystemDirectoryHandle;
    const handle = { getDirectoryHandle: vi.fn(async () => replaysDir) } as unknown as FileSystemDirectoryHandle;
    return { kind: "folder", handle, displayName: "проект" };
  }

  it("создаёт replays/ через дескриптор { create: true } и пишет файл", async () => {
    const written: Record<string, string> = {};
    const source = createFolderSource(new Set(), async (name, text) => {
      written[name] = text;
    });

    const result = await saveRecordingToProject(source, STARTED_AT, RECORDING_TEXT);

    expect(result).toEqual({ ok: true, fileName: "2026-03-04-09-05-07.json" });
    expect(written["2026-03-04-09-05-07.json"]).toBe(RECORDING_TEXT);
    expect(source.kind === "folder" && source.handle.getDirectoryHandle).toHaveBeenCalledWith("replays", { create: true });
  });

  it("имя занято на диске — суффикс -2", async () => {
    const source = createFolderSource(new Set(["2026-03-04-09-05-07.json"]), async () => {});

    const result = await saveRecordingToProject(source, STARTED_AT, RECORDING_TEXT);

    expect(result).toEqual({ ok: true, fileName: "2026-03-04-09-05-07-2.json" });
  });

  it("отозванное разрешение на запись — причина текстом ошибки", async () => {
    const handle = {
      getDirectoryHandle: vi.fn(async () => {
        throw new Error("NotAllowedError");
      }),
    } as unknown as FileSystemDirectoryHandle;
    const source: ProjectSource = { kind: "folder", handle, displayName: "проект" };

    const result = await saveRecordingToProject(source, STARTED_AT, RECORDING_TEXT);

    expect(result).toEqual({ ok: false, reason: "NotAllowedError" });
  });
});
