export type SerialQueue = {
  /** Ставит задачу в очередь — она начинается только после того, как закончились все предыдущие. */
  run<T>(task: () => Promise<T>): Promise<T>;
};

/**
 * Общая очередь для загрузок в один и тот же движок — требование 6 («Редактор»): обычная
 * перезагрузка (внешняя правка) и загрузка по действию редактора не должны звать движок наперегонки,
 * а следующая начинается только после того, как предыдущая совсем закончилась.
 */
export function createSerialQueue(): SerialQueue {
  let tail: Promise<unknown> = Promise.resolve();

  function run<T>(task: () => Promise<T>): Promise<T> {
    const result = tail.then(task, task);
    tail = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  }

  return { run };
}
