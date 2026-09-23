import { isValidGameName } from "../gameName";

export type ProjectNameResolution =
  | { status: "absent" }
  | { status: "invalid"; value: string }
  | { status: "valid"; name: string };

/**
 * `?project=<имя>` открывает проект сразу, минуя выбор — «Редактор», требование 6. Имя проверяется
 * тем же правилом, что и `?game=` у страницы игры (`isValidGameName`): страница знает эти проекты
 * по одному и тому же имени папки в `games/`.
 */
export function resolveProjectName(search: string): ProjectNameResolution {
  const value = new URLSearchParams(search).get("project");
  if (value === null) return { status: "absent" };
  if (!isValidGameName(value)) return { status: "invalid", value };
  return { status: "valid", name: value };
}
