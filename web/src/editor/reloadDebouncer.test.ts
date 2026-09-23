import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createReloadDebouncer } from "./reloadDebouncer";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("createReloadDebouncer", () => {
  it("несколько уведомлений подряд дают одну перезагрузку", async () => {
    const reload = vi.fn(async () => {});
    const debouncer = createReloadDebouncer(reload);

    debouncer.notify();
    await vi.advanceTimersByTimeAsync(100);
    debouncer.notify();
    await vi.advanceTimersByTimeAsync(100);
    debouncer.notify();
    await vi.advanceTimersByTimeAsync(300);

    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("ждёт полные 300 мс после последнего уведомления", async () => {
    const reload = vi.fn(async () => {});
    const debouncer = createReloadDebouncer(reload);

    debouncer.notify();
    await vi.advanceTimersByTimeAsync(299);
    expect(reload).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("уведомление во время идущей перезагрузки даёт ещё одну перезагрузку после неё", async () => {
    let resolveReload: () => void = () => {};
    const reload = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveReload = () => resolve();
        }),
    );
    const debouncer = createReloadDebouncer(reload);

    debouncer.notify();
    await vi.advanceTimersByTimeAsync(300);
    expect(reload).toHaveBeenCalledTimes(1);

    // Изменение пришло, пока первая перезагрузка ещё не завершилась.
    debouncer.notify();
    await vi.advanceTimersByTimeAsync(1000);
    expect(reload).toHaveBeenCalledTimes(1);

    resolveReload();
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(300);

    expect(reload).toHaveBeenCalledTimes(2);
  });

  it("dispose отменяет ещё не начавшуюся перезагрузку", async () => {
    const reload = vi.fn(async () => {});
    const debouncer = createReloadDebouncer(reload);

    debouncer.notify();
    debouncer.dispose();
    await vi.advanceTimersByTimeAsync(1000);

    expect(reload).not.toHaveBeenCalled();
  });

  it("dispose во время идущей перезагрузки не даёт запуститься следующей", async () => {
    let resolveReload: () => void = () => {};
    const reload = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveReload = () => resolve();
        }),
    );
    const debouncer = createReloadDebouncer(reload);

    debouncer.notify();
    await vi.advanceTimersByTimeAsync(300);
    expect(reload).toHaveBeenCalledTimes(1);

    // Изменение пришло, пока перезагрузка ещё не завершилась, а следом за ним — dispose (сценарий
    // «К проектам» во время перезагрузки).
    debouncer.notify();
    debouncer.dispose();

    resolveReload();
    await vi.advanceTimersByTimeAsync(1000);

    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("notify после dispose не планирует перезагрузку", async () => {
    const reload = vi.fn(async () => {});
    const debouncer = createReloadDebouncer(reload);

    debouncer.dispose();
    debouncer.notify();
    await vi.advanceTimersByTimeAsync(1000);

    expect(reload).not.toHaveBeenCalled();
  });

  it("runNow запускает перезагрузку сразу, не дожидаясь дребезга", async () => {
    const reload = vi.fn(async () => {});
    const debouncer = createReloadDebouncer(reload);

    await debouncer.runNow();

    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("runNow во время идущей перезагрузки не запускает вторую поверх неё", async () => {
    let resolveReload: () => void = () => {};
    const reload = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveReload = () => resolve();
        }),
    );
    const debouncer = createReloadDebouncer(reload);

    void debouncer.runNow();
    expect(reload).toHaveBeenCalledTimes(1);

    await debouncer.runNow();
    expect(reload).toHaveBeenCalledTimes(1);

    resolveReload();
    await vi.advanceTimersByTimeAsync(300);
    expect(reload).toHaveBeenCalledTimes(2);
  });
});
