const POLL_INTERVAL_MS = 1000;

export type FileFingerprint = {
  status: number | "network-error";
  etag: string | null;
  lastModified: string | null;
  contentLength: string | null;
};

async function fetchFingerprint(url: string): Promise<FileFingerprint> {
  try {
    const response = await fetch(url, { method: "HEAD", cache: "no-store" });
    return {
      status: response.status,
      etag: response.headers.get("ETag"),
      lastModified: response.headers.get("Last-Modified"),
      contentLength: response.headers.get("Content-Length"),
    };
  } catch {
    return { status: "network-error", etag: null, lastModified: null, contentLength: null };
  }
}

function sameFingerprint(a: FileFingerprint, b: FileFingerprint): boolean {
  return a.status === b.status && a.etag === b.etag && a.lastModified === b.lastModified && a.contentLength === b.contentLength;
}

/**
 * Опрос файлов проекта из списка — «Редактор», требование 32: раз в секунду `HEAD` без кэша по
 * каждому пути, который прочитала последняя загрузка, вместо слежения dev-сервера через HMR — так
 * это работает и на стенде в контейнере, и в `npm run dev`. `fingerprints` — общая таблица
 * путь → отпечаток на весь открытый проект: вызывающий код (`useProjectEngine`) заводит её один раз
 * и передаёт заново при каждом перезапуске опроса, поэтому отпечаток пути переживает пересоздание
 * опроса на следующей загрузке. Так правка, случившаяся между чтением файла загрузкой и первым
 * тиком нового опроса, не теряется — её находит этот же первый тик, сравнивая свежий отпечаток со
 * старым значением в таблице. Молча, без `onChanged`, запоминается только отпечаток пути, которого
 * в таблице раньше не было.
 */
export function pollListedProject(
  baseUrl: string,
  relativePaths: string[],
  fingerprints: Map<string, FileFingerprint>,
  onChanged: () => void,
): () => void {
  let disposed = false;
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function tick(): Promise<void> {
    const results = await Promise.all(
      relativePaths.map(async (path) => [path, await fetchFingerprint(`${baseUrl}${path}`)] as const),
    );
    if (disposed) return;

    let changed = false;
    for (const [path, fingerprint] of results) {
      const previous = fingerprints.get(path);
      if (previous !== undefined && !sameFingerprint(fingerprint, previous)) changed = true;
      fingerprints.set(path, fingerprint);
    }
    if (changed) onChanged();

    // Следующий тик планируется только после того, как этот полностью завершился — `setInterval`
    // запускал бы тики внахлёст при медленном ответе сервера, а тогда ответы могли прийти не по
    // порядку и откатить отпечаток в таблице на устаревший, давая лишние перезагрузки.
    if (!disposed) timer = setTimeout(() => void tick(), POLL_INTERVAL_MS);
  }

  void tick();

  return () => {
    disposed = true;
    if (timer !== null) clearTimeout(timer);
  };
}
