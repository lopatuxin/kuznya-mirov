import type { Engine } from "engine";
import { useEffect, useRef, useState, type Dispatch, type RefObject, type SetStateAction } from "react";
import { createPortal } from "react-dom";
import { computeCanvasLayout } from "../canvasLayout";
import type { EditorCameraStore, FlatCameraStore } from "./editorCamera";
import type { FlatSceneContext } from "./flatSceneController";
import type { HandleMode } from "./handleGeometry";
import { HandleModeToolbar, type TerrainTool } from "./HandleModeToolbar";
import type { MaskSet } from "./maskBytes";
import { IMAGE_DRAG_TYPE, resolveImageDropEffect } from "./imageDrag";
import type { StampShape } from "./imprintGeometry";
import type { PlacementChange } from "./objectPlacement";
import { canPaintMaterial, resolvePaintMaterial } from "./paintLayers";
import type { PaintResult } from "./paintStroke";
import { focusSceneWhenGameStarts } from "./sceneInputDom";
import type { SceneSize } from "./sceneObjects";
import { fitSceneStage } from "./sceneStageLayout";
import {
  isImprintToolEnabled,
  isPaintToolEnabled,
  resolveSceneToolAvailability,
  selectBrushTool,
  selectHandleModeTool,
  selectImprintTool,
  selectPaintTool,
  settleSelectedTool,
  type SelectedTool,
} from "./sceneTools";
import type { SpaceSceneContext } from "./spaceSceneController";
import type { StampPreview } from "./stampPreview";
import { wheelBrushSize } from "./terrainBrush";
import type { ImprintEntry, TerrainCoverLayer, TerrainGrid, TerrainWater } from "./terrainFile";
import { useFlatSceneInput } from "./useFlatSceneInput";
import { useSpaceSceneInput } from "./useSpaceSceneInput";

type SceneCanvasProps = {
  /** Тот же холст, на котором `useProjectEngine` создал движок — требование Engine.create(canvas). */
  canvasRef: RefObject<HTMLCanvasElement | null>;
  engine: Engine | null;
  sceneSize: SceneSize | null;
  /** Ссылка меняется, когда объекты изменились под переносом извне — отменяет его на лету. */
  objectsVersion: unknown;
  /** `scene.json`/живой мир недоступны правке — перенос не начинается (крайний случай). */
  canEditScene: boolean;
  /** Сцена показана: проект загружен без ошибок или идёт партия — иначе ни ручек, ни кнопок их вида («Фаза 16», крайние случаи). */
  isSceneShown: boolean;
  /** Мышь и клавиатура принадлежат игре, а не выбору/переносу — «Редактор», требование 4: партия идёт. */
  isGameInputActive: boolean;
  /** Трёхмерная сцена: камера редактора, выбор лучом, четырёхугольная рамка и ручки («Фаза 16»). Плоская — камера редактора в клетках, рамка по прямоугольнику, ручки переноса и масштаба. */
  isThreeDimensionalScene: boolean;
  /** Сцена вне партии и повтора видна камерой редактора, которую водят мышью («Редактор», «Сцена»). */
  isEditorCameraActive: boolean;
  /** Камера редактора трёхмерной сцены, которую редактор держит и шлёт движку после каждого `show_scene`. */
  editorCameraStore: EditorCameraStore;
  /** То же для плоской сцены. */
  flatCameraStore: FlatCameraStore;
  /** Свойства объекта в виде файла — место, размер, высота, поворот и `shape` для ручек: из текста сцены или из живого мира. */
  getObjectProperties: (objectId: number) => Record<string, unknown> | null;
  selectedIndex: number | null;
  /** Подпись над рамкой выбранного объекта — его имя или номер. */
  selectedLabel: string | null;
  onSelect: (index: number | null) => void;
  /** Отпускание после жеста ручки или переноса объекта: изменившиеся свойства объекта — одно действие. */
  onCommitPlacement: (objectIndex: number, changes: PlacementChange[]) => void;
  /** Картинку из вкладки «Картинки» отпустили над сценой: имя картинки и точка холста («Редактор», требование 25). */
  onDropImage: (imageName: string, x: number, y: number) => void;
  /** Вода рельефа из файла — поля воды над сценой; `null` — воды нет. */
  terrainWater: TerrainWater | null;
  /** Отпускание после мазка кисти: высоты всей сетки — одно действие. */
  onCommitTerrain: (grid: TerrainGrid) => void;
  /** Галочка, уровень или цвет воды: новая вода или `null` — одно действие. */
  onWaterChange: (water: TerrainWater | null) => void;
  /** Мазок начался или кончился — пока он идёт, перезагрузка файлов ждёт. */
  onStrokeActiveChange: (isActive: boolean) => void;
  /** Размер и сила кисти — их держит страница редактора, а не окно проекта («Кисти рельефа», требование 2). */
  brushFields: BrushFields;
  /** Отпечатки файла рельефа как есть, штампы `files.stamps` и выбранный отпечаток — «Лепка рельефа». */
  imprints: readonly ImprintEntry[];
  stampShapes: readonly StampShape[];
  /** Картинки штампов по имени — карточки «Отпечатка» в окошке «Рельеф». */
  stampPreviews: ReadonlyMap<string, StampPreview>;
  selectedImprintIndex: number | null;
  onSelectImprint: (index: number) => void;
  /** Кнопка «Отпечаток» поставила отпечаток: дописывается в конец файла и выбирается. */
  onPlaceImprint: (entry: ImprintEntry) => void;
  /** Отпускание после жеста отпечатка: отпечаток целиком — одно действие. */
  onCommitImprint: (index: number, entry: ImprintEntry) => void;
  /** Материалы `files.materials` в порядке объявления — строки окошка «Материалы». */
  materialNames: readonly string[];
  /** Слои покрытий и их маски из показанных файлов; покрытий нет — `null`. У проекта без файла рельефа `hasTerrainFile` ложно. */
  terrainCovers: readonly TerrainCoverLayer[] | null;
  terrainMasks: MaskSet;
  hasTerrainFile: boolean;
  /** Путь карты цвета `tint` файла рельефа — маска нового слоя его не занимает; карты нет — `null`. */
  terrainTintPath: string | null;
  /** Отпускание после мазка покраски: слои и изменившиеся маски — одно действие. */
  onCommitPaint: (result: PaintResult) => void;
  /** Мазок покраски брошен, а движок нечем вернуть к слоям файла — мир собирается заново. */
  onRestorePaint: () => void;
  /** Место в верхней полосе окна, куда встают инструменты трёхмерной сцены; `null` — полосы ещё нет. */
  toolbarSlot: HTMLElement | null;
  /** Сюда сцена кладёт, чем сразу снять пробный отпечаток, — окно зовёт это перед запуском игры и повтора. */
  clearPreviewRef: RefObject<() => void>;
};

/** Числа нового отпечатка по умолчанию — «Правка сцены», требование 22: ширина и высота в клетках. */
const DEFAULT_IMPRINT_WIDTH = 30;
const DEFAULT_IMPRINT_HEIGHT = 10;

export type BrushFields = {
  size: number;
  strength: number;
  /** Принимает и шаг от последнего значения — Ctrl+колесо. */
  onSizeChange: Dispatch<SetStateAction<number>>;
  onStrengthChange: (strength: number) => void;
};

// Поле вокруг сцены; инструменты трёхмерной сцены — в верхней полосе окна, а не в нём.
const STAGE_PADDING = 28;

/**
 * Холст сцены занимает всю часть окна под неё («Редактор», требование 42) — сцену на нём показывает камера
 * редактора или камера игры, — а рамку выбранного объекта, ручки и границу сцены поверх него рисует сам редактор на
 * втором, прозрачном («Решения» — «рамку выбора рисует редактор, а не движок», требование 24). Щелчок по холсту —
 * `object_at` (требование 23), щелчок по полям вокруг холста снимает выбор. За размером следит
 * `ResizeObserver` на части окна со сценой, а не `window.resize`, как у страницы игры.
 */
export function SceneCanvas({
  canvasRef,
  engine,
  sceneSize,
  objectsVersion,
  canEditScene,
  isSceneShown,
  isGameInputActive,
  isThreeDimensionalScene,
  isEditorCameraActive,
  editorCameraStore,
  flatCameraStore,
  getObjectProperties,
  selectedIndex,
  selectedLabel,
  onSelect,
  onCommitPlacement,
  onDropImage,
  terrainWater,
  onCommitTerrain,
  onWaterChange,
  onStrokeActiveChange,
  brushFields,
  imprints,
  stampShapes,
  stampPreviews,
  selectedImprintIndex,
  onSelectImprint,
  onPlaceImprint,
  onCommitImprint,
  materialNames,
  terrainCovers,
  terrainMasks,
  hasTerrainFile,
  terrainTintPath,
  onCommitPaint,
  onRestorePaint,
  toolbarSlot,
  clearPreviewRef,
}: SceneCanvasProps): React.JSX.Element {
  const areaRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const overlayCanvasRef = useRef<HTMLCanvasElement>(null);
  const isGameInputActiveRef = useRef(isGameInputActive);
  isGameInputActiveRef.current = isGameInputActive;
  const isThreeDimensionalSceneRef = useRef(isThreeDimensionalScene);
  isThreeDimensionalSceneRef.current = isThreeDimensionalScene;
  const [selectedTool, setSelectedTool] = useState<SelectedTool>(selectHandleModeTool("translate"));
  const [imprintStampName, setImprintStampName] = useState<string | null>(null);
  const [imprintWidth, setImprintWidth] = useState(DEFAULT_IMPRINT_WIDTH);
  const [imprintHeight, setImprintHeight] = useState(DEFAULT_IMPRINT_HEIGHT);
  const [paintMaterialName, setPaintMaterialName] = useState<string | null>(null);
  const [lastTerrainTool, setLastTerrainTool] = useState<TerrainTool>("raise");
  const { brushKind } = selectedTool;
  // В плоской сцене поворота нет: вид ручек, оставшийся от трёхмерной сцены, там — перенос.
  const handleMode: HandleMode = !isThreeDimensionalScene && selectedTool.handleMode === "rotate" ? "translate" : selectedTool.handleMode;
  const { size: brushSize, strength: brushStrength } = brushFields;
  const { areHandlesAvailable, areBrushesAvailable } = resolveSceneToolAvailability({
    isThreeDimensionalScene,
    isSceneShown,
    canEditScene,
    isGameInputActive,
    isEditorCameraActive,
  });
  const isImprintEnabled = isImprintToolEnabled(areBrushesAvailable, stampShapes.length > 0);
  // Штамп по умолчанию — первый; объявление, что пропало из `files.stamps`, выбор тоже уводит на первый.
  const imprintStamp = stampShapes.find((shape) => shape.name === imprintStampName) ?? stampShapes[0];
  const isPaintEnabled = isPaintToolEnabled(areBrushesAvailable, materialNames.length > 0);
  const blockedMaterials = new Set(materialNames.filter((name) => !canPaintMaterial(terrainCovers, name)));
  const paintMaterial = resolvePaintMaterial(materialNames, paintMaterialName, blockedMaterials);
  const selectHandleMode = (mode: HandleMode): void => setSelectedTool(selectHandleModeTool(mode));
  const spaceContext: SpaceSceneContext | null =
    engine !== null && isThreeDimensionalScene
      ? {
          engine,
          cameraStore: editorCameraStore,
          isInputLocked: isGameInputActive,
          isEditorCameraActive,
          areHandlesAvailable,
          selectedIndex,
          selectedLabel,
          handleMode,
          brush: areBrushesAvailable && brushKind !== null ? { kind: brushKind, size: brushSize, strength: brushStrength } : null,
          paint:
            selectedTool.isPaintTool && isPaintEnabled && paintMaterial !== undefined && sceneSize !== null
              ? {
                  material: paintMaterial,
                  size: brushSize,
                  strength: brushStrength,
                  covers: terrainCovers,
                  masks: terrainMasks,
                  sceneSize,
                  hasTerrainFile,
                  tintPath: terrainTintPath,
                  onCommit: onCommitPaint,
                  onRestore: onRestorePaint,
                }
              : null,
          getObjectProperties,
          onSelect,
          onHandleModeChange: selectHandleMode,
          onCommitPlacement,
          onCommitTerrain,
          onStrokeActiveChange,
          // Размер считается от последнего, а не от показанного: быстрые щелчки до перерисовки не теряются.
          onBrushSizeWheel: (clicks) => brushFields.onSizeChange((size) => wheelBrushSize(size, clicks)),
          imprints: {
            isEditable: areBrushesAvailable,
            entries: imprints,
            selectedIndex: selectedImprintIndex,
            placing: selectedTool.isImprintTool && isImprintEnabled && imprintStamp !== undefined ? { stamp: imprintStamp, width: imprintWidth, height: imprintHeight } : null,
            onSelect: onSelectImprint,
            onPlace: (entry) => {
              onPlaceImprint(entry);
              selectHandleMode("translate");
            },
            onCommit: onCommitImprint,
            onActiveChange: onStrokeActiveChange,
          },
        }
      : null;
  const flatContext: FlatSceneContext | null =
    engine !== null && !isThreeDimensionalScene
      ? {
          engine,
          cameraStore: flatCameraStore,
          sceneSize,
          isInputLocked: isGameInputActive,
          isEditorCameraActive,
          areHandlesAvailable,
          selectedIndex,
          selectedLabel,
          handleMode,
          getObjectProperties,
          onSelect,
          onHandleModeChange: selectHandleMode,
          onCommitPlacement,
        }
      : null;
  const spaceScene = useSpaceSceneInput({ overlayCanvasRef, context: spaceContext, objectsVersion });
  const flatScene = useFlatSceneInput({ overlayCanvasRef, context: flatContext, objectsVersion });
  useEffect(() => {
    clearPreviewRef.current = spaceScene.clearPreview;
  }, [clearPreviewRef, spaceScene]);
  const canAcceptImages = areHandlesAvailable && !isThreeDimensionalScene;

  // «Запуск» и всё, что убирает кисти или «Отпечаток», — вместо них ручки «Перенос»; после «Стопа» остаются ручки.
  useEffect(() => {
    const settledTool = settleSelectedTool(selectedTool, areBrushesAvailable, isImprintEnabled, isPaintEnabled);
    if (settledTool !== selectedTool) setSelectedTool(settledTool);
  }, [areBrushesAvailable, isImprintEnabled, isPaintEnabled, selectedTool]);

  // Партия пошла — фокус на сцене («Партия в редакторе», требование 33): «Шаг» и повтор `isGameInputActive` не включают.
  useEffect(() => {
    focusSceneWhenGameStarts(overlayCanvasRef.current, isGameInputActive);
  }, [isGameInputActive]);

  // Клавиатура доходит до игры, только когда фокус на холсте, — «Редактор», требование 4.
  useEffect(() => {
    if (!isGameInputActive || !engine) return;
    const activeEngine = engine;
    const overlayCanvas = overlayCanvasRef.current;
    // Отпускание уходит игре, только если до неё дошло нажатие: отпускание сочетания редактора
    // (P от Ctrl+Shift+P) или клавиши, нажатой до фокуса на холсте, игре не принадлежит.
    const pressedCodes = new Set<string>();
    function handleKeyDown(event: KeyboardEvent): void {
      if (document.activeElement !== overlayCanvas) return;
      event.preventDefault();
      pressedCodes.add(event.code);
      activeEngine.key_down(event.code);
    }
    function handleKeyUp(event: KeyboardEvent): void {
      if (!pressedCodes.delete(event.code)) return;
      event.preventDefault();
      activeEngine.key_up(event.code);
    }
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
    };
  }, [isGameInputActive, engine]);

  /**
   * Точка под курсором известна движку только через `mouse_move` — «Мышь в мире», требование 12.
   * Слушатель на самом холсте («Редактор», требование 4) ловит её только пока курсор уже стоит над
   * ним: «Запуск» сбрасывает точку в движке, и до первого движения курсора именно над холстом она
   * неизвестна — щелчок по кнопке экрана сразу после «Запуска» никуда не попадает, а записи с
   * `"cursor"` пропускаются. На `window`, как у страницы игры (`main.ts`),
   * чтобы движение курсора где угодно над окном редактора — не только уже над холстом — обновляло
   * точку раньше, чем придёт нажатие.
   */
  useEffect(() => {
    if (!isGameInputActive || !engine) return;
    const activeEngine = engine;
    function handlePointerMove(event: PointerEvent): void {
      const overlayCanvas = overlayCanvasRef.current;
      if (!overlayCanvas) return;
      const bounds = overlayCanvas.getBoundingClientRect();
      activeEngine.mouse_move(event.clientX - bounds.left, event.clientY - bounds.top);
    }
    window.addEventListener("pointermove", handlePointerMove);
    return () => window.removeEventListener("pointermove", handlePointerMove);
  }, [isGameInputActive, engine]);

  useEffect(() => {
    const area = areaRef.current;
    const stage = stageRef.current;
    const sceneCanvas = canvasRef.current;
    const overlayCanvas = overlayCanvasRef.current;
    if (!engine || !area || !stage || !sceneCanvas || !overlayCanvas) return;
    const activeEngine = engine;

    function applyLayout(areaWidth: number, areaHeight: number): void {
      const stageSize = fitSceneStage(areaWidth, areaHeight, STAGE_PADDING);
      const pixelRatio = window.devicePixelRatio || 1;
      const layout = computeCanvasLayout(stageSize.width, stageSize.height, pixelRatio);
      if (!stage) return;
      stage.style.width = `${layout.cssWidth}px`;
      stage.style.height = `${layout.cssHeight}px`;
      for (const canvas of [sceneCanvas, overlayCanvas]) {
        if (!canvas) continue;
        canvas.style.width = `${layout.cssWidth}px`;
        canvas.style.height = `${layout.cssHeight}px`;
        canvas.width = layout.bufferWidth;
        canvas.height = layout.bufferHeight;
      }
      activeEngine.resize(layout.bufferWidth, layout.bufferHeight);
      activeEngine.set_pixel_ratio(pixelRatio);
      editorCameraStore.refit(activeEngine);
      flatCameraStore.refit(activeEngine);
    }

    applyLayout(area.clientWidth, area.clientHeight);
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) applyLayout(entry.contentRect.width, entry.contentRect.height);
    });
    observer.observe(area);

    return () => observer.disconnect();
  }, [canvasRef, engine, editorCameraStore, flatCameraStore]);

  useEffect(() => {
    if (!engine) return;
    const activeEngine = engine;
    const overlayCanvas = overlayCanvasRef.current;
    const overlayContext = overlayCanvas?.getContext("2d") ?? null;
    let frameHandle = 0;
    let stopped = false;

    function frame(time: number): void {
      if (stopped) return;
      // Мазок действует в каждом кадре страницы и до отрисовки — сцена сразу показывает новый рельеф.
      if (isThreeDimensionalSceneRef.current) spaceScene.strokeFrame(time);
      activeEngine.draw();
      if (overlayCanvas && overlayContext) {
        overlayContext.clearRect(0, 0, overlayCanvas.width, overlayCanvas.height);
        const pixelRatio = window.devicePixelRatio || 1;
        // Нефункциональное требование: пока ничего не выбрано, object_rect не зовётся.
        if (isThreeDimensionalSceneRef.current) spaceScene.draw(overlayContext, pixelRatio);
        else flatScene.draw(overlayContext, pixelRatio);
      }
      frameHandle = requestAnimationFrame(frame);
    }
    frameHandle = requestAnimationFrame(frame);

    return () => {
      stopped = true;
      cancelAnimationFrame(frameHandle);
    };
  }, [engine, spaceScene, flatScene]);

  /**
   * Партия идёт — щелчок по холсту даёт ему фокус и уходит игре, а не выбору («Редактор», требование 4); захват
   * указателя доносит отпускание до холста, даже если кнопку отпустили за ним (фаза 11, требование 15). Вне партии
   * выбор и жесты ведёт контроллер сцены. `mouse_move` шлёт `window`-слушатель выше, а не холст: он не увидел бы
   * движение, начавшееся ещё до входа курсора в холст.
   */
  function handlePointerDown(event: React.PointerEvent<HTMLCanvasElement>): void {
    if (!engine || event.button !== 0 || !isGameInputActiveRef.current) return;
    event.currentTarget.focus();
    event.currentTarget.setPointerCapture(event.pointerId);
    engine.mouse_down();
  }

  function handlePointerUp(event: React.PointerEvent<HTMLCanvasElement>): void {
    // Только левая кнопка доходит до игры (требование 44) — нажатие правой/средней уже не дошло
    // до `mouse_down`, поэтому и её отпускание не должно звонить `mouse_up`.
    if (isGameInputActiveRef.current && event.button === 0) engine?.mouse_up();
  }

  /** Картинку принимает только плоская сцена, когда её можно править: иначе указатель показывает запрет («Редактор», требование 25). */
  function handleDragOver(event: React.DragEvent<HTMLCanvasElement>): void {
    const effect = resolveImageDropEffect(event.dataTransfer.types, canAcceptImages);
    if (effect === null) return;
    event.dataTransfer.dropEffect = effect;
    if (effect === "copy") event.preventDefault();
  }

  function handleDrop(event: React.DragEvent<HTMLCanvasElement>): void {
    if (resolveImageDropEffect(event.dataTransfer.types, canAcceptImages) !== "copy") return;
    event.preventDefault();
    const imageName = event.dataTransfer.getData(IMAGE_DRAG_TYPE);
    if (imageName === "") return;
    // Открытое поле свойства записывается в прежний объект до появления нового — как при смене выбора щелчком.
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
    const bounds = event.currentTarget.getBoundingClientRect();
    onDropImage(imageName, event.clientX - bounds.left, event.clientY - bounds.top);
  }

  function handleAreaClick(event: React.MouseEvent<HTMLDivElement>): void {
    if (event.target === event.currentTarget) onSelect(null);
  }

  return (
    <div ref={areaRef} className="scene-view" onClick={handleAreaClick}>
      <div ref={stageRef} className="scene-view__stage">
        <canvas ref={canvasRef} className="scene-view__world" />
        <canvas
          ref={overlayCanvasRef}
          className="scene-view__overlay"
          tabIndex={-1}
          data-game-input={isGameInputActive ? "true" : undefined}
          onPointerDown={handlePointerDown}
          onPointerUp={handlePointerUp}
          onDragOver={handleDragOver}
          onDrop={handleDrop}
        />
      </div>
      {sceneSize !== null && (
        <span className="scene-view__size">
          Сцена {sceneSize.width} × {sceneSize.height}
        </span>
      )}
      {/* Инструменты сцены — в верхней полосе окна, а не поверх поля сцены. */}
      {toolbarSlot !== null &&
        areHandlesAvailable &&
        createPortal(
          <HandleModeToolbar
            mode={handleMode}
            isThreeDimensionalScene={isThreeDimensionalScene}
            brushKind={brushKind}
            areBrushesAvailable={areBrushesAvailable}
            lastTerrainTool={lastTerrainTool}
            brushSize={brushSize}
            brushStrength={brushStrength}
            water={terrainWater}
            imprintTool={{
              isSelected: selectedTool.isImprintTool,
              isEnabled: isImprintEnabled,
              fields: {
                stampNames: stampShapes.map((shape) => shape.name),
                stamp: imprintStamp?.name ?? "",
                previews: stampPreviews,
                width: imprintWidth,
                height: imprintHeight,
                // Выбор штампа сразу включает «Отпечаток», как выбор материала — кисть этим материалом.
                onStampChange: (name) => {
                  setImprintStampName(name);
                  setLastTerrainTool("imprint");
                  setSelectedTool(selectImprintTool(handleMode));
                },
                onWidthChange: setImprintWidth,
                onHeightChange: setImprintHeight,
              },
              onSelect: () => {
                setLastTerrainTool("imprint");
                setSelectedTool(selectImprintTool(handleMode));
              },
            }}
            materialsTool={{
              isSelected: selectedTool.isPaintTool,
              isEnabled: isPaintEnabled,
              materialNames,
              material: paintMaterial ?? "",
              blockedMaterials,
              onMaterialSelect: (name) => {
                setPaintMaterialName(name);
                setSelectedTool(selectPaintTool(handleMode));
              },
              onSelect: () => setSelectedTool(selectPaintTool(handleMode)),
            }}
            onChange={selectHandleMode}
            onBrushChange={(kind) => {
              setLastTerrainTool(kind);
              setSelectedTool(selectBrushTool(handleMode, kind));
            }}
            onBrushSizeChange={brushFields.onSizeChange}
            onBrushStrengthChange={brushFields.onStrengthChange}
            onWaterChange={onWaterChange}
          />,
          toolbarSlot,
        )}
    </div>
  );
}
