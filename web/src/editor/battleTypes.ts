/**
 * Формы, которые движок отдаёт `any` из вызовов «Редактор», требования 39–47 — здесь они получают
 * настоящие типы, которыми пользуется весь код партии и повтора.
 */

import { formatError, type EngineError } from "../engineErrors";

/** «Редактор», требование 41: живой объект — номер, метка жизни и `name`, если он есть. */
export type WorldObjectSummary = { id: number; generation: number; name: string | null };

/** «Редактор», требование 43. */
export type EngineEditResult = { ok: true } | { ok: false; error: string };

/** «Редактор», требования 18, 43. */
export type EngineAddObjectResult = { ok: true; id: number } | { ok: false; error: string };

/** «Открыть запись»/«Повтор», требования 35, 39, 46. */
export type EngineReplayResult = { ok: true } | { ok: false; error: string };

export type StepReportRuleEntry =
  | { rule: string; kind: "move" | "check" | "delete" | "spawn"; objects: number[] }
  | { rule: string; kind: "collide"; pairs: [number, number][] };

export type StepReportCreated = { id: number; name: string | null; rule: string };

export type DeleteCause = { kind: "rule"; rule: string } | { kind: "code"; function: string; rule: string } | { kind: "lifetime" };

export type StepReportDeleted = { id: number; name: string | null; cause: DeleteCause };

/** «Редактор», требования 23, 44. */
export type StepReport = {
  step: number;
  rules: StepReportRuleEntry[];
  created: StepReportCreated[];
  deleted: StepReportDeleted[];
  screenChange: { from: string; to: string } | null;
  outcome: "win" | "loss" | null;
};

/** «Редактор», требования 26, 45. */
export type SessionMessage = { step: number; text: string };

const RULE_KIND_LABELS: Record<StepReportRuleEntry["kind"], string> = {
  move: "подвинуло",
  check: "проверило",
  collide: "столкнуло",
  delete: "удалило",
  spawn: "создало",
};

/** Подпись вида правила во вкладке «Шаг» — «Редактор», требование 23. */
export function describeRuleKind(kind: StepReportRuleEntry["kind"]): string {
  return RULE_KIND_LABELS[kind];
}

/** Причина удаления объекта во вкладке «Шаг» — «Редактор», требования 23, 44: истёкший срок жизни
 *  движок отдаёт структурой `{kind:"lifetime"}`, готовую фразу строит эта сторона. */
export function describeDeleteCause(cause: DeleteCause): string {
  switch (cause.kind) {
    case "rule":
      return cause.rule;
    case "code":
      return `${cause.function} (${cause.rule})`;
    case "lifetime":
      return "срок жизни истёк";
  }
}

/**
 * Вкладка «Ошибки» партии и повтора — «Редактор», требование 9: ошибка кода игры показывается там
 * же и тем же `formatError`, что и на странице игры, а не только подсказкой значка верхней полосы —
 * `codeError` идёт первой строкой, перед ошибками загрузки самого проекта.
 */
export function withCodeErrorLine(codeError: EngineError | null, errorLines: readonly string[]): string[] {
  return codeError === null ? [...errorLines] : [formatError(codeError), ...errorLines];
}
