import { EditorIcon } from "./EditorIcon";
import { EditorLogoMark } from "./EditorLogoMark";
import type { ProjectSource } from "./projectSource";

type ProjectTopBarProps = {
  projectName: string;
  source: ProjectSource;
  headerNotice: string | null;
  /** Когда проект последний раз загрузился или перезагрузился после правки файлов. */
  loadedAt: Date | null;
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

/** Верхняя полоса окна проекта — «Редактор», требование 8: кнопка «К проектам» и название проекта. */
export function ProjectTopBar({ projectName, source, headerNotice, loadedAt, onBackToProjects }: ProjectTopBarProps): React.JSX.Element {
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
      <div className="project-topbar__status">
        <ProjectStatus headerNotice={headerNotice} loadedAt={loadedAt} />
      </div>
    </header>
  );
}
