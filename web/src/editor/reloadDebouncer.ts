const RELOAD_DEBOUNCE_MS = 300;

export type ReloadDebouncer = {
  /** Уведомление об изменении файла проекта — планирует перезагрузку или откладывает уже идущую. */
  notify(): void;
  /**
   * Перезагрузка сразу, без ожидания дребезга — для самой первой загрузки проекта («Редактор»,
   * требование про немедленное слежение). Перезагрузка уже идёт — ведёт себя как `notify()`:
   * дожидающийся вызов получит те же самые актуальные данные, когда идущая перезагрузка кончится.
   */
  runNow(): Promise<void>;
  /**
   * Снимает таймер и запрещает любую перезагрузку впредь — как ту, что дребезг мог бы запланировать
   * сам после конца уже идущей, так и любой будущий `notify()`/`runNow()`. Перезагрузку, уже идущую
   * в момент вызова, `dispose()` не прерывает — на это отвечает вызывающая сторона.
   */
  dispose(): void;
};

/**
 * Перезагрузка начинается через 300 мс после последнего сообщения об изменении — несколько
 * сохранений подряд дают одну перезагрузку («Редактор», требование 34). Изменение, пришедшее пока
 * `reload()` уже выполняется, не запускает вторую перезагрузку поверх первой — оно откладывается и
 * даёт ещё одну перезагрузку сразу после того, как текущая закончится, если к тому моменту дребезг
 * не был снят `dispose()`.
 */
export function createReloadDebouncer(reload: () => Promise<void>): ReloadDebouncer {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let isReloading = false;
  let notifiedDuringReload = false;
  let disposed = false;

  function clearTimer(): void {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  }

  function scheduleTimer(): void {
    clearTimer();
    timer = setTimeout(() => {
      timer = null;
      void runReload();
    }, RELOAD_DEBOUNCE_MS);
  }

  async function runReload(): Promise<void> {
    isReloading = true;
    try {
      await reload();
    } finally {
      isReloading = false;
    }
    if (disposed) return;
    if (notifiedDuringReload) {
      notifiedDuringReload = false;
      scheduleTimer();
    }
  }

  return {
    notify() {
      if (disposed) return;
      if (isReloading) {
        notifiedDuringReload = true;
        return;
      }
      scheduleTimer();
    },
    runNow() {
      if (disposed) return Promise.resolve();
      if (isReloading) {
        notifiedDuringReload = true;
        return Promise.resolve();
      }
      clearTimer();
      return runReload();
    },
    dispose() {
      disposed = true;
      clearTimer();
    },
  };
}
