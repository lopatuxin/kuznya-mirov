import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { formatError } from "../engineErrors";
import { parseGameDisplayName } from "../gamesIndex";
import { liveObjectListEmptyLabel } from "./battleSelection";
import { isBattleTransportShortcut, isCopyShortcut, isPlayStopShortcut, isReplaySeekShortcut, isStepShortcut, isUndoShortcut } from "./battleShortcuts";
import { withCodeErrorLine } from "./battleTypes";
import { EditorIcon } from "./EditorIcon";
import { ImprintPropertiesPanel } from "./ImprintPropertiesPanel";
import { parseStampShape } from "./imprintGeometry";
import { ObjectList } from "./ObjectList";
import { PanelResizeHandle } from "./PanelResizeHandle";
import { ProblemsTabs } from "./ProblemsTabs";
import { parsePropertyDeclarations } from "./propertiesDeclarations";
import { readTerrainCovers, readTerrainImprints, readTerrainTint, readTerrainWater } from "./terrainFile";
import { parseProjectCellPixels, parseProjectImageDescriptions, parseProjectImageNames, parseProjectMaterialNames } from "./projectFiles";
import { buildImageTiles, createImageObject, imageFrameSize, newObjectParallax, newObjectSize } from "./projectImages";
import { ProjectTopBar } from "./ProjectTopBar";
import { PropertiesPanel } from "./PropertiesPanel";
import { SceneCanvas, type BrushFields } from "./SceneCanvas";
import { stampPreviewOf, type StampPreview } from "./stampPreview";
import { toVec2 } from "./terrainReadings";
import { fallbackDisplayName, type ProjectSource } from "./projectSource";
import {
  buildObjectPropertiesView,
  getObjectProperties,
  parseSceneIsThreeDimensional,
  parseSceneObjects,
  parseSceneSize,
  summarizeSceneObjects,
} from "./sceneObjects";
import { useBattleSession } from "./useBattleSession";
import { useSceneEditing } from "./useSceneEditing";
import { useStoredPanelWidth } from "./useStoredPanelWidth";

type ProjectWindowProps = { source: ProjectSource; onBackToProjects: () => void; brushFields: BrushFields };

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
export function ProjectWindow({ source, onBackToProjects, brushFields }: ProjectWindowProps): React.JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const sceneEditing = useSceneEditing(canvasRef, source);
  const { engine, memory, result, loadedAt, headerNotice, engineError, sceneText, propertiesText, terrainText, saveState, canUndo, selectedIndex, setSelectedIndex, selectedImprintIndex } = sceneEditing;
  const [objectsWidth, setObjectsWidth] = useStoredPanelWidth("kuznya-editor.objects-width", 260);
  const [propertiesWidth, setPropertiesWidth] = useStoredPanelWidth("kuznya-editor.properties-width", 320);
  // Колбэк-реф вместо обычного — «Редактор», партия: элемент нужен движку звука сразу после
  // монтирования, а обычный `useRef` не даёт для этого своего рендера.
  const [audioElement, setAudioElement] = useState<HTMLAudioElement | null>(null);
  // Место в верхней полосе для инструментов сцены — колбэк-реф по той же причине: сцене оно нужно сразу после монтирования полосы.
  const [toolbarSlot, setToolbarSlot] = useState<HTMLElement | null>(null);
  // Чем снять пробный отпечаток со сцены: игра и повтор собирают мир из рельефа, что стоит в движке, а пробного нет в файле.
  const clearScenePreviewRef = useRef<() => void>(() => {});

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
  const isThreeDimensionalScene = useMemo(() => parseSceneIsThreeDimensional(gameJsonText), [gameJsonText]);
  const imageNames = useMemo(() => parseProjectImageNames(gameJsonText), [gameJsonText]);
  const imageDescriptions = useMemo(() => parseProjectImageDescriptions(gameJsonText), [gameJsonText]);
  const cellPixels = useMemo(() => parseProjectCellPixels(gameJsonText), [gameJsonText]);
  const imageTiles = useMemo(() => buildImageTiles(imageDescriptions, result?.status === "ok" ? result.images : []), [imageDescriptions, result]);
  const declaredProperties = useMemo(() => parsePropertyDeclarations(propertiesText), [propertiesText]);
  const terrainWater = useMemo(() => readTerrainWater(terrainText), [terrainText]);
  const imprints = useMemo(() => readTerrainImprints(terrainText), [terrainText]);
  const terrainCovers = useMemo(() => readTerrainCovers(terrainText), [terrainText]);
  const terrainTintPath = useMemo(() => readTerrainTint(terrainText), [terrainText]);
  const materialNames = useMemo(() => parseProjectMaterialNames(gameJsonText), [gameJsonText]);
  const stampShapes = useMemo(() => (result?.status === "ok" ? result.stamps.flatMap(({ name, text }) => parseStampShape(name, text) ?? []) : []), [result]);
  const stampPreviews = useMemo(
    () =>
      new Map<string, StampPreview>(
        result?.status === "ok" ? result.stamps.flatMap(({ name, text }): [string, StampPreview][] => {
          const preview = stampPreviewOf(text);
          return preview === null ? [] : [[name, preview]];
        }) : [],
      ),
    [result],
  );
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
  // Отпечаток выбирается только вне партии, паузы и повтора («Лепка рельефа», «Редактор», требование 26).
  const selectedImprint = !isLive && selectedImprintIndex !== null ? imprints[selectedImprintIndex] : undefined;

  // Свойства для ручек: место, размер, `parallax`, у трёхмерной сцены ещё высота, поворот и `shape` — из текста сцены
  // вне партии, из живого мира на паузе внутри неё («Редактор», требование 20).
  const liveObjectProperties = useCallback((id: number) => (engine?.object_properties(id) as Record<string, unknown> | undefined) ?? null, [engine]);
  const sceneObjectProperties = useCallback((id: number) => getObjectProperties(objects, id), [objects]);

  // Последний selectedIndex и действия правки — в ref, чтобы не переставлять слушатель на каждый рендер.
  const shortcutState = {
    selectedIndex,
    selectedImprintIndex,
    undo: sceneEditing.undo,
    copyObject: sceneEditing.copyObject,
    deleteObject: sceneEditing.deleteObject,
    copyImprint: sceneEditing.copyImprint,
    deleteImprint: sceneEditing.deleteImprint,
  };
  const shortcutStateRef = useRef(shortcutState);
  shortcutStateRef.current = shortcutState;
  const battleRef = useRef(battle);
  battleRef.current = battle;
  const sceneEditingRef = useRef(sceneEditing);
  sceneEditingRef.current = sceneEditing;

  // Мазок кисти держит перезагрузку файлов, пока кнопка нажата («Кисти рельефа», крайние случаи); партия держит её сама.
  // Сочетания, которые пересобирают мир или начинают партию, до конца мазка не срабатывают: иначе мазок
  // бросился бы, а его рельеф остался бы в движке без файла.
  const isStrokeActiveRef = useRef(false);
  const handleStrokeActiveChange = (isActive: boolean): void => {
    isStrokeActiveRef.current = isActive;
    if (battleRef.current.mode === "edit") sceneEditing.setReloadGateOpen(!isActive);
  };

  /**
   * «Запуск» после правок сцены, уже поставленных в очередь: записанное поле или жест перезагружают мир загрузкой
   * по правке, и партия, начатая раньше неё, шла бы на мире, который загрузка тут же подменит. Пока запуск ждёт,
   * второе нажатие ничего не делает.
   */
  const isPlayPendingRef = useRef(false);
  function playAfterSceneEdits(): void {
    if (isPlayPendingRef.current) return;
    isPlayPendingRef.current = true;
    void sceneEditingRef.current.whenIdle().then(() => {
      isPlayPendingRef.current = false;
      // Рендер после загрузки правки ещё не случился, и `sceneAvailable` в нём прежний: неудачную загрузку видно только по движку — мира нет.
      const engine = sceneEditingRef.current.engine;
      if (battleRef.current.mode !== "edit" || isStrokeActiveRef.current || engine === null || !engine.has_world()) return;
      battleRef.current.play();
    });
  }

  // Ctrl+P/Ctrl+Shift+P/Ctrl+Alt+P — «Редактор», требование 1: перехватываются раньше остальных
  // сочетаний и раньше печати браузера, при любом фокусе — фаза перехвата на `window`. Дальше
  // сочетание не идёт: иначе при фокусе на холсте его P дошла бы до игры нажатием клавиши.
  useEffect(() => {
    function handleTransportShortcut(event: KeyboardEvent): void {
      if (!isBattleTransportShortcut(event)) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      if (isStrokeActiveRef.current) return;
      clearScenePreviewRef.current();
      // Открытое поле свойства записывается раньше «Запуска», «Стопа» и паузы, как при нажатии кнопки мышью: на «Запуске»
      // панель свойств пересоздаётся под живой мир, и поле в фокусе пропало бы вместе с черновиком. «Шаг» фокус не трогает.
      const active = document.activeElement;
      const isFieldFocused = active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement || active instanceof HTMLSelectElement;
      if (isFieldFocused && !isStepShortcut(event)) active.blur();
      if (isPlayStopShortcut(event) && battleRef.current.mode === "edit") {
        playAfterSceneEdits();
        return;
      }
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
      if (battleNow.mode === "battle") {
        if (isUndoShortcut(event)) {
          event.preventDefault();
          battleNow.undoLiveEdit();
        } else if (isCopyShortcut(event) && battleNow.liveSelection !== null) {
          event.preventDefault();
          battleNow.copyLiveObject(battleNow.liveSelection.id);
        } else if (event.key === "Delete" && battleNow.liveSelection !== null) {
          event.preventDefault();
          battleNow.deleteLiveObject(battleNow.liveSelection.id);
        }
        return;
      }
      if (battleNow.mode === "replay" || isStrokeActiveRef.current) return;

      if (isUndoShortcut(event)) {
        event.preventDefault();
        current.undo();
      } else if (isCopyShortcut(event)) {
        if (current.selectedImprintIndex !== null) {
          event.preventDefault();
          current.copyImprint(current.selectedImprintIndex);
        } else if (current.selectedIndex !== null) {
          event.preventDefault();
          current.copyObject(current.selectedIndex);
        }
      } else if (event.key === "Delete") {
        if (current.selectedImprintIndex !== null) {
          event.preventDefault();
          current.deleteImprint(current.selectedImprintIndex);
        } else if (current.selectedIndex !== null) {
          event.preventDefault();
          current.deleteObject(current.selectedIndex);
        }
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  // Картинку отпустили на плоскую сцену — «Редактор», «Правка сцены», требования 25–30: новый объект встаёт серединой под
  // указателем, на том же слое, что выбранный; вне партии он дописывается в `scene.json`, на паузе — в живой мир.
  function handleDropImage(imageName: string, x: number, y: number): void {
    const tile = imageTiles.find(({ description }) => description.name === imageName);
    if (engine === null || tile === undefined || tile.image === null) return;
    const selected = displayedSelectedIndex === null ? null : (isLive ? liveObjectProperties : sceneObjectProperties)(displayedSelectedIndex);
    const middle = toVec2(engine.scene_point(x, y, newObjectParallax(selected)));
    if (middle === undefined) return;
    const size = newObjectSize(imageFrameSize(tile.image, tile.description), tile.description, cellPixels);
    const object = createImageObject(imageName, size, middle, selected);
    if (isLive) battle.addLiveObject(object);
    else sceneEditing.addObject(object);
  }

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
  // Кнопки полосы запускают игру и повтор так же, как сочетания: пробный отпечаток сначала снимается.
  const topBarBattle = {
    ...battle,
    play: () => {
      clearScenePreviewRef.current();
      playAfterSceneEdits();
    },
    startReplay: () => {
      clearScenePreviewRef.current();
      battle.startReplay();
    },
    openReplayFile: (text: string) => {
      clearScenePreviewRef.current();
      return battle.openReplayFile(text);
    },
  };

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
        battle={topBarBattle}
        onToolbarSlotChange={setToolbarSlot}
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
          objectsVersion={isLive ? battle.liveObjectSummaries : objects}
          canEditScene={displayedCanEdit}
          isSceneShown={sceneAvailable || isLive}
          isGameInputActive={battle.mode === "battle" && battle.isRunning}
          isThreeDimensionalScene={isThreeDimensionalScene}
          isEditorCameraActive={battle.mode === "edit"}
          editorCameraStore={sceneEditing.editorCameraStore}
          flatCameraStore={sceneEditing.flatCameraStore}
          getObjectProperties={isLive ? liveObjectProperties : sceneObjectProperties}
          selectedIndex={displayedSelectedIndex}
          selectedLabel={displayedSelectedObject === null ? null : (displayedSelectedObject.name ?? `№ ${displayedSelectedObject.index}`)}
          onSelect={displayedOnSelect}
          onCommitPlacement={isLive ? battle.commitLiveTransform : sceneEditing.transformObject}
          onDropImage={handleDropImage}
          terrainWater={terrainWater}
          onCommitTerrain={sceneEditing.paintTerrain}
          onWaterChange={sceneEditing.setTerrainWater}
          onStrokeActiveChange={handleStrokeActiveChange}
          brushFields={brushFields}
          imprints={imprints}
          stampShapes={stampShapes}
          stampPreviews={stampPreviews}
          selectedImprintIndex={isLive ? null : selectedImprintIndex}
          onSelectImprint={sceneEditing.setSelectedImprintIndex}
          onPlaceImprint={sceneEditing.placeImprint}
          onCommitImprint={sceneEditing.replaceImprint}
          materialNames={materialNames}
          terrainCovers={terrainCovers}
          terrainMasks={sceneEditing.masks}
          hasTerrainFile={terrainText !== null}
          terrainTintPath={terrainTintPath}
          onCommitPaint={sceneEditing.paintCovers}
          onRestorePaint={sceneEditing.reloadDisplayed}
          toolbarSlot={toolbarSlot}
          clearPreviewRef={clearScenePreviewRef}
        />
        {!sceneAvailable && !isLive && <ScenePlaceholder isLoading={isLoading} hasEngineFailed={engineError !== null} />}
      </main>

      <aside className="project-window__properties">
        <PanelResizeHandle edge="left" width={propertiesWidth} onWidthChange={setPropertiesWidth} label="Ширина панели свойств" />
        {selectedImprint !== undefined && selectedImprintIndex !== null ? (
          <ImprintPropertiesPanel
            key={`imprint-${selectedImprintIndex}`}
            index={selectedImprintIndex}
            entry={selectedImprint}
            stampNames={stampShapes.map((shape) => shape.name)}
            canEdit={canEdit}
            onSetValue={(key, value) => sceneEditing.setImprintValue(selectedImprintIndex, key, value)}
            onCopy={() => sceneEditing.copyImprint(selectedImprintIndex)}
            onDelete={() => sceneEditing.deleteImprint(selectedImprintIndex)}
          />
        ) : (
          <PropertiesPanel
            key={isLive ? `live-${displayedSelectedIndex ?? "none"}` : (selectedIndex ?? "none")}
            view={displayedPropertiesView}
            selectedObject={displayedSelectedObject}
            canEdit={displayedCanEdit}
            imageNames={imageNames}
            declaredProperties={declaredProperties}
            isThreeDimensionalScene={isThreeDimensionalScene}
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
        )}
      </aside>

      <footer className="project-window__problems">
        <ProblemsTabs
          errorLines={errorLines}
          warningLines={warningLines}
          isLoading={isLoading}
          stepReport={battle.stepReport}
          messages={battle.messages}
          imageTiles={imageTiles}
          onSelectObject={battle.setLiveSelectedId}
        />
      </footer>

      {/* Музыка партии и повтора — «Редактор», требование 1: теми же модулями `web/src/sound`, что у страницы игры. */}
      <audio ref={setAudioElement} hidden />
    </div>
  );
}
