import { useEffect, useMemo, useRef } from "react";
import { formatError } from "../engineErrors";
import { parseGameDisplayName } from "../gamesIndex";
import { EditorIcon } from "./EditorIcon";
import { ErrorsPanel } from "./ErrorsPanel";
import { ObjectList } from "./ObjectList";
import { PanelResizeHandle } from "./PanelResizeHandle";
import { parsePropertyDeclarations } from "./propertiesDeclarations";
import { parseProjectImageNames } from "./projectFiles";
import { ProjectTopBar } from "./ProjectTopBar";
import { PropertiesPanel } from "./PropertiesPanel";
import { SceneCanvas } from "./SceneCanvas";
import { fallbackDisplayName, type ProjectSource } from "./projectSource";
import { buildObjectPropertiesView, parseSceneObjects, parseSceneSize, summarizeSceneObjects } from "./sceneObjects";
import { useSceneEditing } from "./useSceneEditing";
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
 * Поле ввода в фокусе — «Редактор», требование 19: глобальные клавиши там ведут себя как обычно у
 * поля. Галочка, выпадающий список и палитра цвета своих клавиш для Ctrl+D/Delete/Ctrl+Z не держат
 * — фокус на них глобальные клавиши не гасит, иначе кнопка сразу после щелчка по галочке не отвечала бы.
 */
function isEditableElementFocused(): boolean {
  const active = document.activeElement;
  if (active === null) return false;
  if (active instanceof HTMLTextAreaElement) return true;
  if (active instanceof HTMLInputElement) {
    return active.type !== "checkbox" && active.type !== "color";
  }
  return (active as HTMLElement).isContentEditable;
}

/**
 * Окно открытого проекта — «Редактор», требование 8: полоса сверху, список/сцена/свойства в
 * три колонки, ошибки во всю ширину снизу; каждая панель прокручивается сама по себе. Боковые
 * панели автор растягивает мышью за их внутренний край.
 */
export function ProjectWindow({ source, onBackToProjects }: ProjectWindowProps): React.JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const sceneEditing = useSceneEditing(canvasRef, source);
  const { engine, result, loadedAt, headerNotice, engineError, sceneText, propertiesText, saveState, canUndo, selectedIndex, setSelectedIndex } = sceneEditing;
  const [objectsWidth, setObjectsWidth] = useStoredPanelWidth("kuznya-editor.objects-width", 260);
  const [propertiesWidth, setPropertiesWidth] = useStoredPanelWidth("kuznya-editor.properties-width", 320);

  const gameJsonText = result && result.status !== "entry-missing" ? result.gameJsonText : null;
  const projectName = displayNameFor(source, gameJsonText);
  const sceneAvailable = result?.status === "ok";
  // Ни результата загрузки, ни отказа движка ещё нет — самая первая загрузка проекта ещё идёт
  // («Редактор», требование про «Загрузка…» вместо «Сцены нет» и «Ошибок и предупреждений нет»).
  const isLoading = result === null && engineError === null;
  // Текст сцены известен (правка держит его в памяти) — панель свойств и перенос мышью доступны, даже
  // если сама игра не запускается из-за ошибки в другом файле (крайний случай: сломан rules.json).
  const canEdit = sceneText !== null && propertiesText !== null;

  const objects = useMemo(() => parseSceneObjects(sceneText), [sceneText]);
  const objectSummaries = useMemo(() => summarizeSceneObjects(objects), [objects]);
  const propertiesView = useMemo(() => buildObjectPropertiesView(objects, selectedIndex), [objects, selectedIndex]);
  const sceneSize = useMemo(() => parseSceneSize(gameJsonText), [gameJsonText]);
  const imageNames = useMemo(() => parseProjectImageNames(gameJsonText), [gameJsonText]);
  const declaredProperties = useMemo(() => parsePropertyDeclarations(propertiesText), [propertiesText]);
  const selectedObject = selectedIndex !== null ? (objectSummaries[selectedIndex] ?? null) : null;

  // Последний selectedIndex и действия правки — в ref, чтобы не переставлять слушатель на каждый рендер.
  const shortcutStateRef = useRef({ selectedIndex, undo: sceneEditing.undo, copyObject: sceneEditing.copyObject, deleteObject: sceneEditing.deleteObject });
  shortcutStateRef.current = { selectedIndex, undo: sceneEditing.undo, copyObject: sceneEditing.copyObject, deleteObject: sceneEditing.deleteObject };

  // Ctrl+D, Delete и Ctrl+Z — «Редактор», требование 19: не тогда, когда фокус в поле ввода.
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent): void {
      if (isEditableElementFocused()) return;
      const current = shortcutStateRef.current;
      if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "z") {
        event.preventDefault();
        current.undo();
      } else if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "d") {
        if (current.selectedIndex === null) return;
        event.preventDefault();
        current.copyObject(current.selectedIndex);
      } else if (event.key === "Delete") {
        if (current.selectedIndex === null) return;
        event.preventDefault();
        current.deleteObject(current.selectedIndex);
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

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
        saveState={saveState}
        canUndo={canUndo}
        onUndo={sceneEditing.undo}
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
          objects={objects}
          canEditScene={canEdit}
          selectedIndex={selectedIndex}
          selectedLabel={selectedObject === null ? null : (selectedObject.name ?? `№ ${selectedObject.index}`)}
          onSelect={setSelectedIndex}
          onMoveObject={sceneEditing.moveObject}
        />
        {!sceneAvailable && <ScenePlaceholder isLoading={isLoading} hasEngineFailed={engineError !== null} />}
      </main>

      <aside className="project-window__properties">
        <PanelResizeHandle edge="left" width={propertiesWidth} onWidthChange={setPropertiesWidth} label="Ширина панели свойств" />
        <PropertiesPanel
          key={selectedIndex ?? "none"}
          view={propertiesView}
          selectedObject={selectedObject}
          canEdit={canEdit}
          imageNames={imageNames}
          declaredProperties={declaredProperties}
          onSetValue={(key, value) => selectedIndex !== null && sceneEditing.setPropertyValue(selectedIndex, key, value)}
          onRemove={(key) => selectedIndex !== null && sceneEditing.removeProperty(selectedIndex, key)}
          onAdd={(key, value) => selectedIndex !== null && sceneEditing.addProperty(selectedIndex, key, value)}
          onDeclare={(key, kind, value) => selectedIndex !== null && sceneEditing.declareProperty(selectedIndex, key, kind, value)}
          onCopy={() => selectedIndex !== null && sceneEditing.copyObject(selectedIndex)}
          onDelete={() => selectedIndex !== null && sceneEditing.deleteObject(selectedIndex)}
        />
      </aside>

      <footer className="project-window__problems">
        <ErrorsPanel errorLines={errorLines} warningLines={warningLines} isLoading={isLoading} />
      </footer>
    </div>
  );
}
