import { useEffect, useState } from "react";
import { loadGameOptions, type GameOption, type GameOptionsResult } from "../gameOptions";
import { EditorIcon } from "./EditorIcon";
import { EditorLogoMark } from "./EditorLogoMark";
import type { ProjectSource } from "./projectSource";

type ProjectSelectionProps = { onOpenFolder: (source: ProjectSource) => void };

type LoadState =
  | { status: "loading" }
  | { status: "loaded"; result: GameOptionsResult }
  | { status: "error"; message: string };

const CAN_OPEN_FOLDER = typeof window !== "undefined" && typeof window.showDirectoryPicker === "function";
const SKELETON_CARD_COUNT = 3;

function ProjectCard({ option }: { option: GameOption }): React.JSX.Element {
  return (
    <a className="project-card" href={`?project=${encodeURIComponent(option.id)}`}>
      <span className="project-card__monogram">{option.name.slice(0, 1).toUpperCase()}</span>
      <span className="project-card__body">
        <span className="project-card__name">{option.name}</span>
        <span className="project-card__path">games/{option.id}</span>
      </span>
      <EditorIcon name="arrow-right" size={16} className="project-card__arrow" />
    </a>
  );
}

/**
 * Список проектов из `games/index.json` — «Редактор», требования 4–7. Список читается тем же
 * кодом, что у страницы игры (`loadGameOptions`); недоступный `games/index.json` — сообщение
 * вместо списка, кнопка «Открыть папку» остаётся. Карточка проекта — обычная ссылка на
 * `?project=<имя>`, её можно открыть и в новой вкладке.
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
    <main className="editor-home">
      <div className="editor-home__inner">
        <header className="editor-home__header">
          <EditorLogoMark size={48} />
          <div>
            <h1 className="editor-home__title">
              Кузня Миров <span className="editor-home__title-suffix">— редактор</span>
            </h1>
            <p className="editor-home__lead">Откройте проект, чтобы увидеть его сцену, объекты и ошибки проверки.</p>
          </div>
        </header>

        <section className="editor-home__section">
          <h2 className="editor-home__section-title">
            Проекты
            {state.status === "loaded" && <span className="editor-count">{state.result.options.length}</span>}
          </h2>

          {state.status === "error" && (
            <div className="editor-alert editor-alert--error">
              <EditorIcon name="error" size={16} />
              {state.message}
            </div>
          )}
          {state.status === "loaded" && state.result.failures.length > 0 && (
            <div className="editor-alert editor-alert--warning">
              <EditorIcon name="warning" size={16} />
              Не открылись: {state.result.failures.map((failure) => `${failure.id} (${failure.reason})`).join("; ")}
            </div>
          )}

          <div className="project-grid">
            {state.status === "loading" &&
              Array.from({ length: SKELETON_CARD_COUNT }, (_, index) => (
                <div key={index} className="project-card project-card--skeleton" aria-hidden="true">
                  <span className="project-card__monogram" />
                  <span className="project-card__body">
                    <span className="skeleton-line skeleton-line--wide" />
                    <span className="skeleton-line" />
                  </span>
                </div>
              ))}
            {state.status === "loaded" && state.result.options.map((option) => <ProjectCard key={option.id} option={option} />)}

            <button type="button" className="project-card project-card--folder" disabled={!CAN_OPEN_FOLDER} onClick={() => void handleOpenFolder()}>
              <span className="project-card__monogram">
                <EditorIcon name="folder" size={20} />
              </span>
              <span className="project-card__body">
                <span className="project-card__name">Открыть папку</span>
                <span className="project-card__path">{CAN_OPEN_FOLDER ? "проект с диска" : "работает в Chrome и Edge"}</span>
              </span>
            </button>
          </div>
          {state.status === "loading" && <p className="editor-home__status">Загрузка списка проектов…</p>}
        </section>
      </div>
    </main>
  );
}
