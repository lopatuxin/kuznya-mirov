import type { EngineError } from "../engineErrors";

/** Русские подписи свойств бега и прыжка в тексте ошибки движка — «Редактор», Фаза 37, требование 26. */
const RUN_JUMP_LABELS = {
  run_speed: "скорость бега",
  jump_height: "высота прыжка",
  jump_time: "время подъёма",
  on_ground: "стоит на земле",
} as const;

/** Ключ в кавычках: движок называет свойство так — «свойство "run_speed" …», «признак "on_ground" …», — и подпись встаёт в фразу, не ломая род. */
const RUN_JUMP_KEY_PATTERN = new RegExp(`"(${Object.keys(RUN_JUMP_LABELS).join("|")})"`, "g");

/** Ошибка движка с русскими подписями вместо ключей бега и прыжка в тексте; место в файле (`path`) остаётся как есть — по нему ищут строку. */
export function withRunJumpLabels(error: EngineError): EngineError {
  return {
    ...error,
    message: error.message.replace(RUN_JUMP_KEY_PATTERN, (_quoted, key: keyof typeof RUN_JUMP_LABELS) => `"${RUN_JUMP_LABELS[key]}"`),
  };
}
