const NO_OBSERVER_NOTICE = "Браузер не сообщает об изменениях в папке — изменения не подхватываются.";
const OBSERVER_ERRORED_NOTICE = "Папка больше не отслеживается — откройте её заново.";

/**
 * `replays/` — запись партий («Редактор», требование 34), не часть файлов, которые редактор
 * загружает: правка в ней (в том числе появление самой папки на первой записи) не должна выглядеть
 * как правка проекта и вызывать перезагрузку («Редактор», требование 36).
 */
export function hasProjectFileChange(relativePaths: readonly (readonly string[])[]): boolean {
  return relativePaths.some((components) => components[0] !== "replays");
}

/**
 * Слежение за папкой с диска через `FileSystemObserver`, рекурсивно — «Редактор», требование 33.
 * Любая запись об изменении вне `replays/`, включая `unknown`, даёт одну перезагрузку на пачку
 * записей; запись `errored` останавливает слежение вместо неё. Браузер без `FileSystemObserver` —
 * папка всё равно открывается, просто без слежения.
 */
export function watchFolderProject(
  handle: FileSystemDirectoryHandle,
  onChanged: () => void,
  onNotice: (notice: string) => void,
): () => void {
  if (typeof FileSystemObserver === "undefined") {
    onNotice(NO_OBSERVER_NOTICE);
    return () => {};
  }

  const observer = new FileSystemObserver((records) => {
    if (records.some((record) => record.type === "errored")) {
      onNotice(OBSERVER_ERRORED_NOTICE);
      observer.disconnect();
      return;
    }
    if (hasProjectFileChange(records.map((record) => record.relativePathComponents))) onChanged();
  });

  observer.observe(handle, { recursive: true }).catch(() => {
    onNotice(NO_OBSERVER_NOTICE);
  });

  return () => observer.disconnect();
}
