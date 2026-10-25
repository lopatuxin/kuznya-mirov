import type { EngineError } from "./engineErrors";

/** `engine.tick()`'s raw shape — «Код игры» → «API движка для страницы (wasm)». */
export type RawTickResult =
  | { running: true }
  | { running: false; outcome: "win" | "loss"; step: number }
  | { running: false; error: EngineError }
  | { running: false; exit: true };

export type TickResult =
  | { status: "running" }
  | { status: "ended"; outcome: "win" | "loss"; step: number }
  | { status: "error"; error: EngineError }
  | { status: "exit" };

/**
 * Разбирает сырой ответ `engine.tick()` в размеченное объединение: кадровый цикл решает, что
 * делать дальше, по одному полю `status`, а не заново проверяет `running`/`outcome`/`error`/`exit` сам.
 * Код ошибки приоритетнее `exit` («Экраны и состояние» → «Команды», `exit`).
 */
export function parseTickResult(raw: RawTickResult): TickResult {
  if (raw.running) return { status: "running" };
  if ("error" in raw) return { status: "error", error: raw.error };
  if ("exit" in raw) return { status: "exit" };
  return { status: "ended", outcome: raw.outcome, step: raw.step };
}
