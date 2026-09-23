/**
 * `showDirectoryPicker` и `FileSystemObserver` — Chrome и Edge, не в `lib.dom.d.ts` TypeScript.
 * `FileSystemDirectoryHandle`/`FileSystemFileHandle` там уже есть — их не объявляем заново.
 */

interface DirectoryPickerOptions {
  id?: string;
  mode?: "read" | "readwrite";
}

interface Window {
  showDirectoryPicker?(options?: DirectoryPickerOptions): Promise<FileSystemDirectoryHandle>;
}

type FileSystemObserverChangeType = "appeared" | "disappeared" | "modified" | "moved" | "unknown" | "errored";

interface FileSystemChangeRecord {
  type: FileSystemObserverChangeType;
  root: FileSystemHandle;
  changedHandle: FileSystemHandle;
  relativePathComponents: string[];
}

interface FileSystemObserverObserveOptions {
  recursive?: boolean;
}

declare class FileSystemObserver {
  constructor(callback: (records: FileSystemChangeRecord[], observer: FileSystemObserver) => void);
  observe(handle: FileSystemHandle, options?: FileSystemObserverObserveOptions): Promise<void>;
  unobserve(handle: FileSystemHandle): void;
  disconnect(): void;
}
