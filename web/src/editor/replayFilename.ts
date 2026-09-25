function pad(value: number): string {
  return String(value).padStart(2, "0");
}

/**
 * Имя файла записи — «Редактор», требование 34: `ГГГГ-ММ-ДД-ЧЧ-ММ-СС.json` по местному времени
 * начала партии (`play()`/`replay()`), не времени сохранения.
 */
export function formatReplayTimestamp(startedAt: Date): string {
  const year = startedAt.getFullYear();
  const month = pad(startedAt.getMonth() + 1);
  const day = pad(startedAt.getDate());
  const hours = pad(startedAt.getHours());
  const minutes = pad(startedAt.getMinutes());
  const seconds = pad(startedAt.getSeconds());
  return `${year}-${month}-${day}-${hours}-${minutes}-${seconds}`;
}

/** `replays/<timestamp>.json`, с суффиксом `-N` при попытке номер `N` — требование 34: две записи в одну секунду. */
export function replayFileNameForAttempt(timestamp: string, attempt: number): string {
  return attempt === 1 ? `${timestamp}.json` : `${timestamp}-${attempt}.json`;
}

/**
 * Первое свободное имя записи — пробует `<timestamp>.json`, потом `-2`, `-3`… пока `exists` не
 * скажет «нет». `exists` сам решает, как проверить (`HEAD` для проекта из списка, `getFileHandle`
 * для папки с диска) — здесь только порядок попыток.
 */
export async function pickAvailableReplayFileName(timestamp: string, exists: (fileName: string) => Promise<boolean>): Promise<string> {
  for (let attempt = 1; attempt < 1000; attempt++) {
    const fileName = replayFileNameForAttempt(timestamp, attempt);
    if (!(await exists(fileName))) return fileName;
  }
  // Практически недостижимо — 1000 записей одной секунды; вызывающая сторона просто перезапишет последнюю.
  return replayFileNameForAttempt(timestamp, 1);
}
