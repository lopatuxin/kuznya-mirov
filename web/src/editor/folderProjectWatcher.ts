const NO_OBSERVER_NOTICE = "Браузер не сообщает об изменениях в папке — изменения не подхватываются.";
const OBSERVER_ERRORED_NOTICE = "Папка больше не отслеживается — откройте её заново.";

/**
 * Слежение за папкой с диска через `FileSystemObserver`, рекурсивно — «Редактор», требование 33.
 * Любая запись об изменении, включая `unknown`, даёт одну перезагрузку на пачку записей; запись
 * `errored` останавливает слежение вместо неё. Браузер без `FileSystemObserver` — папка всё равно
 * открывается, просто без слежения.
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
    onChanged();
  });

  observer.observe(handle, { recursive: true }).catch(() => {
    onNotice(NO_OBSERVER_NOTICE);
  });

  return () => observer.disconnect();
}
