import { useEffect, useMemo, useRef, useState } from "react";
import { formatError } from "../engineErrors";
import { parseGameDisplayName } from "../gamesIndex";
import { ErrorsPanel } from "./ErrorsPanel";
import { ObjectList } from "./ObjectList";
import { PropertiesPanel } from "./PropertiesPanel";
import { SceneCanvas } from "./SceneCanvas";
import { fallbackDisplayName, type ProjectSource } from "./projectSource";
import { buildObjectPropertiesView, parseSceneObjects, resolveSelectionAfterReload, summarizeSceneObjects } from "./sceneObjects";
import { useProjectEngine } from "./useProjectEngine";

type ProjectWindowProps = { source: ProjectSource; onBackToProjects: () => void };

function displayNameFor(source: ProjectSource, gameJsonText: string | null): string {
  if (gameJsonText !== null) {
    try {
      return parseGameDisplayName(gameJsonText);
    } catch {
      // падает на своё — имя проекта из списка или папки, требование 9.
    }
  }
  return fallbackDisplayName(source);
}

/**
 * Окно открытого проекта — «Редактор», требование 8: полоса сверху, список/сцена/свойства в
 * три колонки, ошибки во всю ширину снизу; каждая панель прокручивается сама по себе.
 */
export function ProjectWindow({ source, onBackToProjects }: ProjectWindowProps): React.JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const { engine, result, headerNotice, engineError } = useProjectEngine(canvasRef, source);
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);

  const gameJsonText = result && result.status !== "entry-missing" ? result.gameJsonText : null;
  const sceneText = result && result.status !== "entry-missing" ? result.sceneText : null;
  const projectName = displayNameFor(source, gameJsonText);
  const sceneAvailable = result?.status === "ok";
  // Ни результата загрузки, ни отказа движка ещё нет — самая первая загрузка проекта ещё идёт
  // («Редактор», требование про «Загрузка…» вместо «Сцены нет» и «Ошибок и предупреждений нет»).
  const isLoading = result === null && engineError === null;

  const objects = useMemo(() => parseSceneObjects(sceneText), [sceneText]);
  const objectSummaries = useMemo(() => summarizeSceneObjects(objects), [objects]);
  const propertiesView = useMemo(() => buildObjectPropertiesView(objects, selectedIndex), [objects, selectedIndex]);

  // После перезагрузки выбран объект с тем же номером, если он есть в новом `scene.json`; иначе
  // выбор снят («Редактор», требование 36). Эффект, а не запись состояния прямо во время рендера.
  useEffect(() => {
    setSelectedIndex((current) => resolveSelectionAfterReload(current, objects.length));
  }, [objects]);

  const errorLines = [
    ...(engineError !== null ? [engineError] : []),
    ...(result === null
      ? []
      : result.status === "entry-missing"
        ? [`Нет файла game.json в проекте «${fallbackDisplayName(source)}»`]
        : result.status === "rejected"
          ? result.errors.map(formatError)
          : []),
  ];
  const warningLines = result && result.status !== "entry-missing" ? result.warnings.map(formatError) : [];

  return (
    <div className="project-window">
      <header className="project-window__header">
        <button type="button" className="project-window__back" onClick={onBackToProjects}>
          ← К проектам
        </button>
        <span className="project-window__title">{projectName}</span>
        {headerNotice !== null && <span className="project-window__notice">{headerNotice}</span>}
      </header>

      <div className="project-window__list">
        <ObjectList objects={objectSummaries} selectedIndex={selectedIndex} onSelect={setSelectedIndex} />
      </div>

      <div className="project-window__scene">
        <SceneCanvas canvasRef={canvasRef} engine={engine} selectedIndex={selectedIndex} onSelect={setSelectedIndex} />
        {!sceneAvailable && (
          <div className="project-window__scene-placeholder">{isLoading ? "Загрузка…" : "Сцены нет — в проекте ошибки"}</div>
        )}
      </div>

      <div className="project-window__properties">
        <PropertiesPanel view={propertiesView} />
      </div>

      <div className="project-window__errors">
        <ErrorsPanel errorLines={errorLines} warningLines={warningLines} isLoading={isLoading} />
      </div>
    </div>
  );
}
