import { EditorIcon } from "./EditorIcon";
import { EditorLogoMark } from "./EditorLogoMark";
import type { SaveState } from "./editSession";
import type { ProjectSource } from "./projectSource";

type ProjectTopBarProps = {
  projectName: string;
  source: ProjectSource;
  headerNotice: string | null;
  /** Когда проект последний раз загрузился или перезагрузился после правки файлов. */
  loadedAt: Date | null;
  saveState: SaveState;
  canUndo: boolean;
  onUndo: () => void;
  onBackToProjects: () => void;
};

function ProjectStatus({ headerNotice, loadedAt }: Pick<ProjectTopBarProps, "headerNotice" | "loadedAt">): React.JSX.Element | null {
  if (headerNotice !== null) {
    return (
      <span className="project-topbar__notice">
        <EditorIcon name="warning" size={14} />
        {headerNotice}
      </span>
    );
  }
  if (loadedAt === null) return null;
  // `key` по времени перезапускает вспышку строки на каждой перезагрузке — видно, что правка подхвачена.
  return (
    <span key={loadedAt.getTime()} className="project-topbar__live project-topbar__live--flash" title="Редактор сам перечитывает проект, когда его файлы меняются">
      <span className="project-topbar__live-dot" />
      Обновлено в {loadedAt.toLocaleTimeString("ru-RU")}
    </span>
  );
}

/** «Не сохранено — …» — «Редактор», требования 2, 4: строка появляется, пока показанный текст не записан. */
function SaveStateNotice({ saveState }: { saveState: SaveState }): React.JSX.Element | null {
  if (saveState.status === "saved") return null;
  return (
    <span className="project-topbar__notice" title={saveState.reason}>
      <EditorIcon name="warning" size={14} />
      Не сохранено — {saveState.reason}
    </span>
  );
}

/**
 * Верхняя полоса окна проекта — «Редактор», требование 8: кнопка «К проектам» и название проекта.
 * «Отменить» и строка «Не сохранено» — требования 2, 4, 21: кнопка неактивна без истории.
 */
export function ProjectTopBar({ projectName, source, headerNotice, loadedAt, saveState, canUndo, onUndo, onBackToProjects }: ProjectTopBarProps): React.JSX.Element {
  return (
    <header className="project-topbar">
      <EditorLogoMark size={26} />
      <button type="button" className="editor-button" onClick={onBackToProjects}>
        <EditorIcon name="arrow-left" size={15} />К проектам
      </button>
      <span className="project-topbar__divider" />
      <h1 className="project-topbar__title">{projectName}</h1>
      <span className="editor-chip" title={source.kind === "listed" ? "Проект из списка games/" : "Папка с диска"}>
        {source.kind === "listed" ? `games/${source.name}` : (
          <>
            <EditorIcon name="folder" size={12} />
            {source.displayName}
          </>
        )}
      </span>
      <button type="button" className="editor-button" disabled={!canUndo} title="Отменить (Ctrl+Z)" onClick={onUndo}>
        <EditorIcon name="undo" size={15} />
        Отменить
      </button>
      <div className="project-topbar__status">
        <SaveStateNotice saveState={saveState} />
        <ProjectStatus headerNotice={headerNotice} loadedAt={loadedAt} />
      </div>
    </header>
  );
}
