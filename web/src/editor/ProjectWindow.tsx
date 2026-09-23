import { useEffect, useMemo, useRef, useState } from "react";
import { formatError } from "../engineErrors";
import { parseGameDisplayName } from "../gamesIndex";
import { EditorIcon } from "./EditorIcon";
import { ErrorsPanel } from "./ErrorsPanel";
import { ObjectList } from "./ObjectList";
import { PanelResizeHandle } from "./PanelResizeHandle";
import { ProjectTopBar } from "./ProjectTopBar";
import { PropertiesPanel } from "./PropertiesPanel";
import { SceneCanvas } from "./SceneCanvas";
import { fallbackDisplayName, type ProjectSource } from "./projectSource";
import {
  buildObjectPropertiesView,
  parseSceneObjects,
  parseSceneSize,
  resolveSelectionAfterReload,
  summarizeSceneObjects,
} from "./sceneObjects";
import { useProjectEngine } from "./useProjectEngine";
import { useStoredPanelWidth } from "./useStoredPanelWidth";

type ProjectWindowProps = { source: ProjectSource; onBackToProjects: () => void };

const PANEL_MIN_SHRUNK_WIDTH = 170;
const SCENE_MIN_WIDTH = 320;

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

type ScenePlaceholderProps = {
  isLoading: boolean;
  /** Движок не запустился — слежения за файлами нет, и обещать подхват правок нельзя. */
  hasEngineFailed: boolean;
};

function ScenePlaceholder({ isLoading, hasEngineFailed }: ScenePlaceholderProps): React.JSX.Element {
  if (isLoading) {
    return (
      <div className="scene-placeholder">
        <span className="editor-spinner editor-spinner--large" />
        Загрузка…
      </div>
    );
  }
  return (
    <div className="scene-placeholder scene-placeholder--error">
      <EditorIcon name="error" size={28} />
      <span className="scene-placeholder__title">Сцены нет — в проекте ошибки</span>
      {!hasEngineFailed && (
        <span className="scene-placeholder__hint">Исправьте файлы проекта — редактор подхватит сохранённые изменения сам.</span>
      )}
    </div>
  );
}

/**
 * Окно открытого проекта — «Редактор», требование 8: полоса сверху, список/сцена/свойства в
 * три колонки, ошибки во всю ширину снизу; каждая панель прокручивается сама по себе. Боковые
 * панели автор растягивает мышью за их внутренний край.
 */
export function ProjectWindow({ source, onBackToProjects }: ProjectWindowProps): React.JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const { engine, result, loadedAt, headerNotice, engineError } = useProjectEngine(canvasRef, source);
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
  const [objectsWidth, setObjectsWidth] = useStoredPanelWidth("kuznya-editor.objects-width", 260);
  const [propertiesWidth, setPropertiesWidth] = useStoredPanelWidth("kuznya-editor.properties-width", 320);

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
  const sceneSize = useMemo(() => parseSceneSize(gameJsonText), [gameJsonText]);
  const selectedObject = selectedIndex !== null ? (objectSummaries[selectedIndex] ?? null) : null;

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
    <div
      className="project-window"
      // Панели держат свою ширину, пока сцене хватает места; в узком окне первыми сжимаются они, а не сцена.
      style={{
        gridTemplateColumns: `minmax(${PANEL_MIN_SHRUNK_WIDTH}px, ${objectsWidth}px) minmax(${SCENE_MIN_WIDTH}px, 1fr) minmax(${PANEL_MIN_SHRUNK_WIDTH}px, ${propertiesWidth}px)`,
      }}
    >
      <ProjectTopBar
        projectName={projectName}
        source={source}
        headerNotice={headerNotice}
        loadedAt={loadedAt}
        onBackToProjects={onBackToProjects}
      />

      <aside className="project-window__objects">
        <ObjectList objects={objectSummaries} selectedIndex={selectedIndex} onSelect={setSelectedIndex} />
        <PanelResizeHandle edge="right" width={objectsWidth} onWidthChange={setObjectsWidth} label="Ширина списка объектов" />
      </aside>

      <main className="project-window__scene">
        <SceneCanvas
          canvasRef={canvasRef}
          engine={engine}
          sceneSize={sceneSize}
          selectedIndex={selectedIndex}
          selectedLabel={selectedObject === null ? null : (selectedObject.name ?? `№ ${selectedObject.index}`)}
          onSelect={setSelectedIndex}
        />
        {!sceneAvailable && <ScenePlaceholder isLoading={isLoading} hasEngineFailed={engineError !== null} />}
      </main>

      <aside className="project-window__properties">
        <PanelResizeHandle edge="left" width={propertiesWidth} onWidthChange={setPropertiesWidth} label="Ширина панели свойств" />
        <PropertiesPanel view={propertiesView} selectedObject={selectedObject} />
      </aside>

      <footer className="project-window__problems">
        <ErrorsPanel errorLines={errorLines} warningLines={warningLines} isLoading={isLoading} />
      </footer>
    </div>
  );
}
