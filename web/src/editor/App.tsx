import { useMemo, useState } from "react";
import { EditorIcon } from "./EditorIcon";
import { ProjectSelection } from "./ProjectSelection";
import { ProjectWindow } from "./ProjectWindow";
import { resolveProjectName } from "./projectName";
import type { ProjectSource } from "./projectSource";

function backToProjects(): void {
  location.search = "";
}

/**
 * Верхнеуровневая раскладка редактора — «Редактор», требования 4–10: выбор проекта из
 * `?project=<имя>`, папка с диска (в состоянии, не в адресе — требование 7) или окно проекта.
 */
export function App(): React.JSX.Element {
  const [folderSource, setFolderSource] = useState<ProjectSource | null>(null);
  const resolution = resolveProjectName(location.search);
  const listedProjectName = resolution.status === "valid" ? resolution.name : null;
  // `useProjectEngine` перезапускает эффект (освобождает и заводит движок заново) по идентичности
  // `source` — без мемоизации новый объект на каждый рендер делал бы это на ровном месте.
  const listedSource = useMemo<ProjectSource | null>(
    () => (listedProjectName === null ? null : { kind: "listed", name: listedProjectName }),
    [listedProjectName],
  );

  if (folderSource !== null) {
    return <ProjectWindow source={folderSource} onBackToProjects={() => setFolderSource(null)} />;
  }

  switch (resolution.status) {
    case "absent":
      return <ProjectSelection onOpenFolder={setFolderSource} />;
    case "invalid":
      return (
        <main className="editor-home">
          <div className="editor-home__inner editor-home__inner--narrow">
            <div className="editor-alert editor-alert--error">
              <EditorIcon name="error" size={16} />
              Параметр project=«{resolution.value}» недопустим — разрешены только латинские буквы, цифры, «_» и «-».
            </div>
            <button type="button" className="editor-button editor-button--outline" onClick={backToProjects}>
              <EditorIcon name="arrow-left" size={15} />К проектам
            </button>
          </div>
        </main>
      );
    case "valid":
      // `listedProjectName` не `null` именно потому, что `resolution.status === "valid"` — `listedSource` не `null` тоже.
      return <ProjectWindow source={listedSource as ProjectSource} onBackToProjects={backToProjects} />;
  }
}
