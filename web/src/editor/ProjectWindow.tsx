import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { formatError } from "../engineErrors";
import { parseGameDisplayName } from "../gamesIndex";
import { liveObjectListEmptyLabel } from "./battleSelection";
import { isBattleTransportShortcut, isReplaySeekShortcut } from "./battleShortcuts";
import { withCodeErrorLine } from "./battleTypes";
import { EditorIcon } from "./EditorIcon";
import { ObjectList } from "./ObjectList";
import { PanelResizeHandle } from "./PanelResizeHandle";
import { ProblemsTabs } from "./ProblemsTabs";
import { parsePropertyDeclarations } from "./propertiesDeclarations";
import { parseProjectImageNames } from "./projectFiles";
import { ProjectTopBar } from "./ProjectTopBar";
import { PropertiesPanel } from "./PropertiesPanel";
import { SceneCanvas } from "./SceneCanvas";
import { fallbackDisplayName, type ProjectSource } from "./projectSource";
import { buildObjectPropertiesView, getObjectGeometry, parseSceneObjects, parseSceneSize, summarizeSceneObjects } from "./sceneObjects";
import { useBattleSession } from "./useBattleSession";
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
 * Партия идёт и фокус на холсте — клавиши тоже принадлежат игре, не редактору («Редактор», требование 4).
 */
function isEditableElementFocused(): boolean {
  const active = document.activeElement;
  if (active === null) return false;
  if (active instanceof HTMLTextAreaElement) return true;
  if (active instanceof HTMLInputElement) {
    return active.type !== "checkbox" && active.type !== "color";
  }
  if (active instanceof HTMLElement && active.dataset.gameInput === "true") return true;
  return (active as HTMLElement).isContentEditable;
}

/**
 * Окно открытого проекта — «Редактор», требование 8: полоса сверху, список/сцена/свойства в
 * три колонки, ошибки во всю ширину снизу; каждая панель прокручивается сама по себе. Боковые
 * панели автор растягивает мышью за их внутренний край. Партия, пауза и повтор («Редактор», фаза 10)
 * подменяют живыми данными из движка то, что вне них список и свойства берут из текста файлов.
 */
export function ProjectWindow({ source, onBackToProjects }: ProjectWindowProps): React.JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const sceneEditing = useSceneEditing(canvasRef, source);
  const { engine, memory, result, loadedAt, headerNotice, engineError, sceneText, propertiesText, saveState, canUndo, selectedIndex, setSelectedIndex } = sceneEditing;
  const [objectsWidth, setObjectsWidth] = useStoredPanelWidth("kuznya-editor.objects-width", 260);
  const [propertiesWidth, setPropertiesWidth] = useStoredPanelWidth("kuznya-editor.properties-width", 320);
  // Колбэк-реф вместо обычного — «Редактор», партия: элемент нужен движку звука сразу после
  // монтирования, а обычный `useRef` не даёт для этого своего рендера.
  const [audioElement, setAudioElement] = useState<HTMLAudioElement | null>(null);

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

  const battle = useBattleSession({
    engine,
    memory,
    source,
    sceneAvailable,
    loadedSounds: result?.status === "ok" ? result.loadedSounds : [],
    musicTracks: result?.status === "ok" ? result.musicTracks : [],
    audioContext: result?.status === "ok" ? result.audioContext : null,
    audioElement,
    sceneObjectCount: objects.length,
    editSelectedIndex: selectedIndex,
    onEditSelectionChange: setSelectedIndex,
    hasQueuedReload: sceneEditing.hasQueuedReload,
    setReloadGateOpen: sceneEditing.setReloadGateOpen,
  });

  const isLive = battle.mode !== "edit";
  const displayedObjectSummaries = isLive ? battle.liveObjectSummaries : objectSummaries;
  const displayedSelectedIndex = isLive ? (battle.liveSelection?.id ?? null) : selectedIndex;
  const displayedSelectedObject = isLive ? battle.liveSelectedSummary : selectedObject;
  const displayedPropertiesView = isLive ? battle.livePropertiesView : propertiesView;
  const displayedCanEdit = isLive ? battle.canLiveEdit : canEdit;
  const displayedOnSelect = isLive ? battle.setLiveSelectedId : setSelectedIndex;

  // Место и размер объекта по номеру для переноса мышью — из текста сцены вне партии, из живого
  // мира на паузе внутри неё («Редактор», требование 20).
  const liveObjectGeometry = useCallback(
    (id: number) => {
      const properties = engine?.object_properties(id) as Record<string, unknown> | undefined;
      const position = properties?.position;
      const size = properties?.size;
      if (!Array.isArray(position) || !Array.isArray(size)) return null;
      return { position: [position[0], position[1]] as const, size: [size[0], size[1]] as const };
    },
    [engine],
  );
  const sceneObjectGeometry = useCallback((id: number) => getObjectGeometry(objects, id), [objects]);

  // Последний selectedIndex и действия правки — в ref, чтобы не переставлять слушатель на каждый рендер.
  const shortcutStateRef = useRef({ selectedIndex, undo: sceneEditing.undo, copyObject: sceneEditing.copyObject, deleteObject: sceneEditing.deleteObject });
  shortcutStateRef.current = { selectedIndex, undo: sceneEditing.undo, copyObject: sceneEditing.copyObject, deleteObject: sceneEditing.deleteObject };
  const battleRef = useRef(battle);
  battleRef.current = battle;

  // Ctrl+P/Ctrl+Shift+P/Ctrl+Alt+P — «Редактор», требование 1: перехватываются раньше остальных
  // сочетаний и раньше печати браузера, при любом фокусе — фаза перехвата на `window`. Дальше
  // сочетание не идёт: иначе при фокусе на холсте его P дошла бы до игры нажатием клавиши.
  useEffect(() => {
    function handleTransportShortcut(event: KeyboardEvent): void {
      if (!isBattleTransportShortcut(event)) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      battleRef.current.handleShortcut(event);
    }
    window.addEventListener("keydown", handleTransportShortcut, true);
    return () => window.removeEventListener("keydown", handleTransportShortcut, true);
  }, []);

  // Ctrl+D, Delete, Ctrl+Z — «Редактор», требование 19 (правка сцены) и требование 21 (правка на
  // ходу партии, вместо файловой); ← и → — шкала повтора, требование 29. Ни то ни другое не тогда,
  // когда фокус в поле ввода или, во время идущей партии, на холсте.
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent): void {
      if (isEditableElementFocused()) return;
      const battleNow = battleRef.current;

      if (battleNow.mode === "replay") {
        const seekDirection = isReplaySeekShortcut(event);
        if (seekDirection === "back") {
          event.preventDefault();
          battleNow.stepBack();
          return;
        }
        if (seekDirection === "forward") {
          // «Редактор», требование 30: шаг вперёд — тот же `step()`, что и кнопка «Шаг» (применяет
          // ввод следующего шага и шагает), не `seek(currentStep + 1)` — пересчёт с начала ради
          // одного шага вперёд стоил бы секунды на длинной записи (нефункциональное требование).
          // Неактивно по той же причине, что «Шаг»: `step_blocked()` не undefined (требование 12) —
          // «Шаг назад» этим не связано, требование 33, он работает и на заблокированном шаге.
          if (battleNow.stepBlockedReason !== undefined) return;
          event.preventDefault();
          battleNow.step();
          return;
        }
      }

      const current = shortcutStateRef.current;
      const isCtrlOnly = event.ctrlKey && !event.shiftKey && !event.altKey;
      if (battleNow.mode === "battle") {
        if (isCtrlOnly && event.key.toLowerCase() === "z") {
          event.preventDefault();
          battleNow.undoLiveEdit();
        } else if (isCtrlOnly && event.key.toLowerCase() === "d" && battleNow.liveSelection !== null) {
          event.preventDefault();
          battleNow.copyLiveObject(battleNow.liveSelection.id);
        } else if (event.key === "Delete" && battleNow.liveSelection !== null) {
          event.preventDefault();
          battleNow.deleteLiveObject(battleNow.liveSelection.id);
        }
        return;
      }
      if (battleNow.mode === "replay") return;

      if (isCtrlOnly && event.key.toLowerCase() === "z") {
        event.preventDefault();
        current.undo();
      } else if (isCtrlOnly && event.key.toLowerCase() === "d") {
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

  const errorLines = withCodeErrorLine(battle.codeError, [
    ...(engineError !== null ? [engineError] : []),
    ...(result === null
      ? []
      : result.status === "entry-missing"
        ? [`Нет файла game.json в проекте «${fallbackDisplayName(source)}»`]
        : result.status === "rejected"
          ? result.errors.map(formatError)
          : []),
  ]);
  const warningLines = result && result.status !== "entry-missing" ? result.warnings.map(formatError) : [];

  return (
    <div
      className={`project-window${isLive ? ` project-window--${battle.mode}` : ""}`}
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
        // «Отменить» откатывает правку на ходу партии, если она идёт, иначе — правку файлов
        // («Редактор», требование 21: у партии своя, отдельная история).
        canUndo={isLive ? battle.canUndoLiveEdit : canUndo}
        onUndo={isLive ? battle.undoLiveEdit : sceneEditing.undo}
        onBackToProjects={onBackToProjects}
        battle={battle}
      />

      <aside className="project-window__objects">
        <ObjectList
          objects={displayedObjectSummaries}
          selectedIndex={displayedSelectedIndex}
          onSelect={displayedOnSelect}
          emptyLabel={isLive ? liveObjectListEmptyLabel(battle.hasWorld) : undefined}
        />
        <PanelResizeHandle edge="right" width={objectsWidth} onWidthChange={setObjectsWidth} label="Ширина списка объектов" />
      </aside>

      <main className="project-window__scene">
        <SceneCanvas
          canvasRef={canvasRef}
          engine={engine}
          sceneSize={sceneSize}
          getObjectGeometry={isLive ? liveObjectGeometry : sceneObjectGeometry}
          objectsVersion={isLive ? battle.liveObjectSummaries : objects}
          canEditScene={displayedCanEdit}
          isGameInputActive={battle.mode === "battle" && battle.isRunning}
          selectedIndex={displayedSelectedIndex}
          selectedLabel={displayedSelectedObject === null ? null : (displayedSelectedObject.name ?? `№ ${displayedSelectedObject.index}`)}
          onSelect={displayedOnSelect}
          onMoveObject={isLive ? battle.commitLiveMove : sceneEditing.moveObject}
        />
        {!sceneAvailable && !isLive && <ScenePlaceholder isLoading={isLoading} hasEngineFailed={engineError !== null} />}
      </main>

      <aside className="project-window__properties">
        <PanelResizeHandle edge="left" width={propertiesWidth} onWidthChange={setPropertiesWidth} label="Ширина панели свойств" />
        <PropertiesPanel
          key={isLive ? `live-${displayedSelectedIndex ?? "none"}` : (selectedIndex ?? "none")}
          view={displayedPropertiesView}
          selectedObject={displayedSelectedObject}
          canEdit={displayedCanEdit}
          imageNames={imageNames}
          declaredProperties={declaredProperties}
          disallowDeclare={isLive}
          onSetValue={(key, value) =>
            isLive
              ? battle.liveSelection !== null && battle.setLiveProperty(battle.liveSelection.id, key, value)
              : selectedIndex !== null && sceneEditing.setPropertyValue(selectedIndex, key, value)
          }
          onRemove={(key) =>
            isLive
              ? battle.liveSelection !== null && battle.removeLiveProperty(battle.liveSelection.id, key)
              : selectedIndex !== null && sceneEditing.removeProperty(selectedIndex, key)
          }
          onAdd={(key, value) => {
            if (isLive) return battle.liveSelection !== null ? battle.setLiveProperty(battle.liveSelection.id, key, value) : undefined;
            if (selectedIndex !== null) sceneEditing.addProperty(selectedIndex, key, value);
            return undefined;
          }}
          onDeclare={(key, kind, value) => selectedIndex !== null && sceneEditing.declareProperty(selectedIndex, key, kind, value)}
          onCopy={() => (isLive ? battle.liveSelection !== null && battle.copyLiveObject(battle.liveSelection.id) : selectedIndex !== null && sceneEditing.copyObject(selectedIndex))}
          onDelete={() =>
            isLive ? battle.liveSelection !== null && battle.deleteLiveObject(battle.liveSelection.id) : selectedIndex !== null && sceneEditing.deleteObject(selectedIndex)
          }
        />
      </aside>

      <footer className="project-window__problems">
        <ProblemsTabs
          errorLines={errorLines}
          warningLines={warningLines}
          isLoading={isLoading}
          stepReport={battle.stepReport}
          messages={battle.messages}
          onSelectObject={battle.setLiveSelectedId}
        />
      </footer>

      {/* Музыка партии и повтора — «Редактор», требование 1: теми же модулями `web/src/sound`, что у страницы игры. */}
      <audio ref={setAudioElement} hidden />
    </div>
  );
}
