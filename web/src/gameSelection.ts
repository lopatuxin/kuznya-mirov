import { isValidGameName } from "./gameName";

export type GameNameResolution =
  | { status: "absent" }
  | { status: "invalid"; value: string }
  | { status: "valid"; name: string };

/**
 * `?game=<name>` в адресе открывает игру сразу, минуя экран выбора. Различает три случая: параметра
 * нет вовсе (страница показывает экран выбора), параметр есть, но не проходит `isValidGameName`
 * (человеку, попросившему конкретную игру, нужно сказать, что не так, а не молча вернуть его на
 * экран выбора), и годное имя.
 */
export function resolveGameName(search: string): GameNameResolution {
  const value = new URLSearchParams(search).get("game");
  if (value === null) return { status: "absent" };
  if (!isValidGameName(value)) return { status: "invalid", value };
  return { status: "valid", name: value };
}
