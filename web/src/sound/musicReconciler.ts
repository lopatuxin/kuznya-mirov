// «Звук» → «Проигрывание на странице»: чистая функция, сравнивающая нужное (объявленное движком плюс
// признак «звук включён») с исполненным (что сейчас лежит в проигрывателе) и решающая, что с
// проигрывателем сделать. Идемпотентна — «имя то же» и «уже играет» дают одно и то же «ничего не
// делать», сколько раз её ни позови подряд.

export type MusicDesired = {
  enabled: boolean;
  trackId: number | null;
};

export type MusicPerformed = {
  trackId: number | null;
  playing: boolean;
};

export type MusicAction =
  | { kind: "none" }
  | { kind: "start"; trackId: number }
  | { kind: "resume" }
  | { kind: "pause" };

export type MusicReconcileOptions = {
  /** «Сломавшийся посреди игры трек молчит до перезагрузки страницы» — второй попытки не будет. */
  brokenTrackIds: ReadonlySet<number>;
  /** «Отложенная попытка» после отказа браузера — до следующего действия игрока сверка не трогает. */
  awaitingRetry: boolean;
};

/**
 * Выключенный звук приравнивается к тишине тем же движением, каким её объявляет экран без музыки, —
 * не отдельная ветка, а частный случай «нужного». Сравниваются должное с исполненным, а не новый
 * экран со старым, поэтому сверка чинит расхождение, откуда бы оно ни взялось — включая паузу,
 * которую поставил сам браузер (спрятанная вкладка, потерянный вывод) мимо этой функции.
 */
export function reconcileMusic(
  desired: MusicDesired,
  performed: MusicPerformed,
  options: MusicReconcileOptions,
): MusicAction {
  if (options.awaitingRetry) return { kind: "none" };

  const targetTrackId = desired.enabled ? desired.trackId : null;

  // Сломанный трек звучит так же, как и его отсутствие, — «на экране со сломанным треком тишина».
  // Если сейчас играет ДРУГОЙ трек (пришли из партии в меню, а музыка меню сломана), его требуется
  // остановить, а не оставить играть в пустоту до бесконечности.
  if (targetTrackId === null || options.brokenTrackIds.has(targetTrackId)) {
    return performed.playing ? { kind: "pause" } : { kind: "none" };
  }

  if (performed.trackId === targetTrackId) {
    return performed.playing ? { kind: "none" } : { kind: "resume" };
  }

  return { kind: "start", trackId: targetTrackId };
}
