import { useEffect, useState } from "react";
import { loadGameOptions, type GameOptionsResult } from "../gameOptions";
import type { ProjectSource } from "./projectSource";

type ProjectSelectionProps = { onOpenFolder: (source: ProjectSource) => void };

type LoadState =
  | { status: "loading" }
  | { status: "loaded"; result: GameOptionsResult }
  | { status: "error"; message: string };

const CAN_OPEN_FOLDER = typeof window !== "undefined" && typeof window.showDirectoryPicker === "function";

/**
 * Список проектов из `games/index.json` — «Редактор», требования 4–7. Список читается тем же
 * кодом, что у страницы игры (`loadGameOptions`); недоступный `games/index.json` — сообщение
 * вместо списка, кнопка «Открыть папку» остаётся.
 */
export function ProjectSelection({ onOpenFolder }: ProjectSelectionProps): React.JSX.Element {
  const [state, setState] = useState<LoadState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    loadGameOptions()
      .then((result) => {
        if (!cancelled) setState({ status: "loaded", result });
      })
      .catch((error: unknown) => {
        if (!cancelled) setState({ status: "error", message: error instanceof Error ? error.message : String(error) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function handleOpenFolder(): Promise<void> {
    if (!window.showDirectoryPicker) return;
    let handle: FileSystemDirectoryHandle;
    try {
      handle = await window.showDirectoryPicker({ mode: "read" });
    } catch {
      // Отмена окна выбора ничего не меняет — требование 7.
      return;
    }
    onOpenFolder({ kind: "folder", handle, displayName: handle.name });
  }

  return (
    <main className="project-selection">
      <h1>Кузня Миров — редактор</h1>

      {state.status === "loading" && <p className="project-selection__status">Загрузка списка проектов…</p>}
      {state.status === "error" && <p className="project-selection__status">{state.message}</p>}
      {state.status === "loaded" && (
        <>
          <div className="project-selection__list">
            {state.result.options.map((option) => (
              <button
                key={option.id}
                type="button"
                className="project-option"
                onClick={() => {
                  location.search = `?project=${encodeURIComponent(option.id)}`;
                }}
              >
                {option.name}
              </button>
            ))}
          </div>
          {state.result.failures.length > 0 && (
            <p className="project-selection__failures">
              Не открылись: {state.result.failures.map((failure) => `${failure.id} (${failure.reason})`).join("; ")}
            </p>
          )}
        </>
      )}

      <div className="project-selection__folder">
        <button type="button" className="project-option" disabled={!CAN_OPEN_FOLDER} onClick={() => void handleOpenFolder()}>
          Открыть папку
        </button>
        {!CAN_OPEN_FOLDER && <span className="project-selection__folder-note">работает в Chrome и Edge</span>}
      </div>
    </main>
  );
}
