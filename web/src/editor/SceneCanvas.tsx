import type { Engine } from "engine";
import { useEffect, useRef, useState, type RefObject } from "react";
import { computeCanvasLayout } from "../canvasLayout";
import { cellSizeFromObjectRect, computeDragPosition, hasCrossedDragThreshold } from "./dragPlacement";
import type { EditorCameraStore } from "./editorCamera";
import type { HandleMode } from "./handleGeometry";
import { HandleModeToolbar } from "./HandleModeToolbar";
import type { MaskSet } from "./maskBytes";
import type { StampShape } from "./mountainGeometry";
import type { PlacementChange } from "./objectPlacement";
import { canPaintMaterial } from "./paintLayers";
import type { PaintResult } from "./paintStroke";
import type { SceneSize } from "./sceneObjects";
import { drawSelection, type CanvasRect } from "./selectionDrawing";
import { fitSceneStage } from "./sceneStageLayout";
import {
  isMountainToolEnabled,
  isPaintToolEnabled,
  resolveSceneToolAvailability,
  selectBrushTool,
  selectHandleModeTool,
  selectMountainTool,
  selectPaintTool,
  settleSelectedTool,
  type SelectedTool,
} from "./sceneTools";
import type { SpaceSceneContext } from "./spaceSceneController";
import type { MountainEntry, TerrainCoverLayer, TerrainGrid, TerrainWater } from "./terrainFile";
import { useSpaceSceneInput } from "./useSpaceSceneInput";

type ObjectGeometry = { position: readonly [number, number]; size: readonly [number, number] };

type SceneCanvasProps = {
  /** Тот же холст, на котором `useProjectEngine` создал движок — требование Engine.create(canvas). */
  canvasRef: RefObject<HTMLCanvasElement | null>;
  engine: Engine | null;
  sceneSize: SceneSize | null;
  /**
   * Место и размер объекта по его номеру, для геометрии переноса (требование 8) — из текста
   * `scene.json` в правке, из живого мира в партии на паузе («Редактор», требование 20).
   */
  getObjectGeometry: (objectId: number) => ObjectGeometry | null;
  /** Ссылка меняется, когда объекты изменились под переносом извне — отменяет его на лету. */
  objectsVersion: unknown;
  /** `scene.json`/живой мир недоступны правке — перенос не начинается (крайний случай). */
  canEditScene: boolean;
  /** Сцена показана: проект загружен без ошибок или идёт партия — иначе ни ручек, ни кнопок их вида («Фаза 16», крайние случаи). */
  isSceneShown: boolean;
  /** Мышь и клавиатура принадлежат игре, а не выбору/переносу — «Редактор», требование 4: партия идёт. */
  isGameInputActive: boolean;
  /**
   * В партии, на паузе и в повторе холст занимает всю часть окна под сцену, как страница игры —
   * «Редактор», требование 42; трёхмерная сцена занимает её и вне партии — камера сама вписывает
   * землю в холст («Фаза 15», требование 27); плоская вне партии вписывается по своим пропорциям.
   */
  fillsStageArea: boolean;
  /** Трёхмерная сцена: камера редактора, выбор лучом, четырёхугольная рамка и ручки («Фаза 16»). Плоская — как раньше. */
  isThreeDimensionalScene: boolean;
  /** Сцена вне партии и повтора видна камерой редактора, которую водят мышью («Редактор», «Сцена»). */
  isEditorCameraActive: boolean;
  /** Камера редактора, которую редактор держит и шлёт движку после каждого `show_scene`. */
  editorCameraStore: EditorCameraStore;
  /** Свойства объекта в виде файла — место, размер, высота, поворот и `shape` для ручек: из текста сцены или из живого мира. */
  getObjectProperties: (objectId: number) => Record<string, unknown> | null;
  selectedIndex: number | null;
  /** Подпись над рамкой выбранного объекта — его имя или номер. */
  selectedLabel: string | null;
  onSelect: (index: number | null) => void;
  /**
   * Отпускание после переноса — «Редактор», требование 9: одно действие с новым местом объекта.
   * `previousPosition` — место на начало переноса, для отмены правки на ходу (требование 21): его
   * не восстановить из движка в момент отпускания — `move_object` уже успел передвинуть объект
   * там во время самого переноса, так что «текущее» свойство к этому моменту и есть новое место.
   */
  onMoveObject: (objectIndex: number, position: readonly [number, number], previousPosition: readonly [number, number]) => void;
  /** Отпускание после жеста ручки или переноса по земле в трёхмерной сцене: изменившиеся свойства объекта — одно действие. */
  onCommitPlacement: (objectIndex: number, changes: PlacementChange[]) => void;
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
  /** Горы файла рельефа как есть, штампы `files.stamps` и выбранная гора — «Лепка рельефа». */
  mountains: readonly MountainEntry[];
  stampShapes: readonly StampShape[];
  selectedMountainIndex: number | null;
  onSelectMountain: (index: number) => void;
  /** Кнопка «Гора» поставила гору: дописывается в конец файла и выбирается. */
  onPlaceMountain: (entry: MountainEntry) => void;
  /** Отпускание после жеста горы: гора целиком — одно действие. */
  onCommitMountain: (index: number, entry: MountainEntry) => void;
  /** Материалы `files.materials` в порядке объявления — поле «Материал» кнопки «Покрасить». */
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
};

/** Числа новой горы по умолчанию — «Правка сцены», требование 22: ширина и высота в клетках. */
const DEFAULT_MOUNTAIN_WIDTH = 30;
const DEFAULT_MOUNTAIN_HEIGHT = 10;

export type BrushFields = {
  size: number;
  strength: number;
  onSizeChange: (size: number) => void;
  onStrengthChange: (strength: number) => void;
};

type DragState = {
  objectIndex: number;
  pointerId: number;
  startClientX: number;
  startClientY: number;
  startPosition: readonly [number, number];
  cellSizePx: number;
  hasStartedDrag: boolean;
  lastPosition: readonly [number, number];
};

// Поле вокруг сцены; у трёхмерной сверху два ряда инструментов, поэтому поле шире.
const STAGE_PADDING = 28;
const SPACE_STAGE_PADDING = 60;

/**
 * Сцена вписана в свою часть окна с сохранением пропорций («Редактор», требование 22) на одном
 * холсте, а рамку выбранного объекта поверх него рисует сам редактор на втором, прозрачном
 * («Решения» — «рамку выбора рисует редактор, а не движок», требование 24). Щелчок по холсту —
 * `object_at` (требование 23), щелчок по полям вокруг сцены снимает выбор. За размером следит
 * `ResizeObserver` на части окна со сценой, а не `window.resize`, как у страницы игры.
 */
export function SceneCanvas({
  canvasRef,
  engine,
  sceneSize,
  getObjectGeometry,
  objectsVersion,
  canEditScene,
  isSceneShown,
  isGameInputActive,
  fillsStageArea,
  isThreeDimensionalScene,
  isEditorCameraActive,
  editorCameraStore,
  getObjectProperties,
  selectedIndex,
  selectedLabel,
  onSelect,
  onMoveObject,
  onCommitPlacement,
  terrainWater,
  onCommitTerrain,
  onWaterChange,
  onStrokeActiveChange,
  brushFields,
  mountains,
  stampShapes,
  selectedMountainIndex,
  onSelectMountain,
  onPlaceMountain,
  onCommitMountain,
  materialNames,
  terrainCovers,
  terrainMasks,
  hasTerrainFile,
  terrainTintPath,
  onCommitPaint,
  onRestorePaint,
}: SceneCanvasProps): React.JSX.Element {
  const areaRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const overlayCanvasRef = useRef<HTMLCanvasElement>(null);
  const selectedIndexRef = useRef(selectedIndex);
  selectedIndexRef.current = selectedIndex;
  const selectedLabelRef = useRef(selectedLabel);
  selectedLabelRef.current = selectedLabel;
  const getObjectGeometryRef = useRef(getObjectGeometry);
  getObjectGeometryRef.current = getObjectGeometry;
  const canEditSceneRef = useRef(canEditScene);
  canEditSceneRef.current = canEditScene;
  const isGameInputActiveRef = useRef(isGameInputActive);
  isGameInputActiveRef.current = isGameInputActive;
  const isThreeDimensionalSceneRef = useRef(isThreeDimensionalScene);
  isThreeDimensionalSceneRef.current = isThreeDimensionalScene;
  const onMoveObjectRef = useRef(onMoveObject);
  onMoveObjectRef.current = onMoveObject;
  const dragRef = useRef<DragState | null>(null);
  const [selectedTool, setSelectedTool] = useState<SelectedTool>(selectHandleModeTool("translate"));
  const [mountainStampName, setMountainStampName] = useState<string | null>(null);
  const [mountainWidth, setMountainWidth] = useState(DEFAULT_MOUNTAIN_WIDTH);
  const [mountainHeight, setMountainHeight] = useState(DEFAULT_MOUNTAIN_HEIGHT);
  const [paintMaterialName, setPaintMaterialName] = useState<string | null>(null);
  const { handleMode, brushKind } = selectedTool;
  const { size: brushSize, strength: brushStrength } = brushFields;
  const { areHandlesAvailable, areBrushesAvailable } = resolveSceneToolAvailability({
    isThreeDimensionalScene,
    isSceneShown,
    canEditScene,
    isGameInputActive,
    isEditorCameraActive,
  });
  const isMountainEnabled = isMountainToolEnabled(areBrushesAvailable, stampShapes.length > 0);
  // Штамп по умолчанию — первый; объявление, что пропало из `files.stamps`, выбор тоже уводит на первый.
  const mountainStamp = stampShapes.find((shape) => shape.name === mountainStampName) ?? stampShapes[0];
  const isPaintEnabled = isPaintToolEnabled(areBrushesAvailable, materialNames.length > 0);
  // Материал по умолчанию — первый; объявление, что пропало из `files.materials`, выбор тоже уводит на первый.
  const paintMaterial = materialNames.find((name) => name === paintMaterialName) ?? materialNames[0];
  const blockedMaterials = new Set(materialNames.filter((name) => !canPaintMaterial(terrainCovers, name)));
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
          mountains: {
            isEditable: areBrushesAvailable,
            entries: mountains,
            selectedIndex: selectedMountainIndex,
            placing: selectedTool.isMountainTool && isMountainEnabled && mountainStamp !== undefined ? { stamp: mountainStamp, width: mountainWidth, height: mountainHeight } : null,
            onSelect: onSelectMountain,
            onPlace: (entry) => {
              onPlaceMountain(entry);
              selectHandleMode("translate");
            },
            onCommit: onCommitMountain,
            onActiveChange: onStrokeActiveChange,
          },
        }
      : null;
  const spaceScene = useSpaceSceneInput({ overlayCanvasRef, context: spaceContext, objectsVersion });
  const sceneWidth = sceneSize?.width ?? null;
  const sceneHeight = sceneSize?.height ?? null;

  // «Запуск» и всё, что убирает кисти или «Гору», — вместо них ручки «Перенос»; после «Стопа» остаются ручки.
  useEffect(() => {
    const settledTool = settleSelectedTool(selectedTool, areBrushesAvailable, isMountainEnabled, isPaintEnabled);
    if (settledTool !== selectedTool) setSelectedTool(settledTool);
  }, [areBrushesAvailable, isMountainEnabled, isPaintEnabled, selectedTool]);

  // Внешняя правка или другое действие поменяли объекты во время переноса — «Редактор», крайний
  // случай: перенос отменяется, мир движок уже собрал заново из показанного своей перезагрузкой.
  useEffect(() => {
    dragRef.current = null;
  }, [objectsVersion]);

  // Партия пошла или встала на паузу — начатый мышью перенос больше не имеет смысла в новом режиме.
  useEffect(() => {
    dragRef.current = null;
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
    const knownSceneSize =
      fillsStageArea || sceneWidth === null || sceneHeight === null ? null : { width: sceneWidth, height: sceneHeight };

    function applyLayout(areaWidth: number, areaHeight: number): void {
      const stageSize = fitSceneStage(areaWidth, areaHeight, knownSceneSize, isThreeDimensionalScene ? SPACE_STAGE_PADDING : STAGE_PADDING);
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
    }

    applyLayout(area.clientWidth, area.clientHeight);
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) applyLayout(entry.contentRect.width, entry.contentRect.height);
    });
    observer.observe(area);

    return () => observer.disconnect();
  }, [canvasRef, engine, sceneWidth, sceneHeight, fillsStageArea, isThreeDimensionalScene, editorCameraStore]);

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
        const selected = selectedIndexRef.current;
        // Нефункциональное требование: пока ничего не выбрано, object_rect не зовётся.
        if (isThreeDimensionalSceneRef.current) {
          spaceScene.draw(overlayContext, window.devicePixelRatio || 1);
        } else if (selected !== null) {
          const rect = activeEngine.object_rect(selected) as CanvasRect | undefined;
          if (rect) drawSelection(overlayContext, rect, selectedLabelRef.current, window.devicePixelRatio || 1);
        }
      }
      frameHandle = requestAnimationFrame(frame);
    }
    frameHandle = requestAnimationFrame(frame);

    return () => {
      stopped = true;
      cancelAnimationFrame(frameHandle);
    };
  }, [engine, spaceScene]);

  /**
   * Нажатие выбирает объект под указателем — «Редактор», требование 7 (выбор на нажатии, не на
   * отпускании). Правка недоступна или под указателем нет объекта с `position`/`size` в тексте —
   * перенос не заводится, но выбор всё равно работает (крайний случай: `scene.json` не разобрать).
   */
  function handlePointerDown(event: React.PointerEvent<HTMLCanvasElement>): void {
    if (!engine || event.button !== 0) return;
    // Партия идёт — щелчок по холсту даёт ему фокус и уходит игре, а не выбору («Редактор», требование 4).
    // Захват указателя доносит отпускание до холста, даже если кнопку отпустили за ним (фаза 11, требование 15).
    if (isGameInputActiveRef.current) {
      event.currentTarget.focus();
      event.currentTarget.setPointerCapture(event.pointerId);
      engine.mouse_down();
      return;
    }
    if (isThreeDimensionalSceneRef.current) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const hit = engine.object_at(event.clientX - bounds.left, event.clientY - bounds.top) as number | undefined;
    // Открытое поле свойства записывается в прежний объект (требование 13) до смены выбора: иначе
    // панель пересоздаётся под новый номер раньше, чем браузер снимет фокус, и черновик пропадёт.
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
    onSelect(typeof hit === "number" ? hit : null);
    if (typeof hit !== "number" || !canEditSceneRef.current) return;

    const geometry = getObjectGeometryRef.current(hit);
    const rect = engine.object_rect(hit) as { width: number } | undefined;
    if (geometry === null || !rect) return;

    dragRef.current = {
      objectIndex: hit,
      pointerId: event.pointerId,
      startClientX: event.clientX,
      startClientY: event.clientY,
      startPosition: geometry.position,
      cellSizePx: cellSizeFromObjectRect(rect.width, geometry.size[0]),
      hasStartedDrag: false,
      lastPosition: geometry.position,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  /**
   * Партия идёт — указатель над холстом принадлежит игре целиком («Редактор», требование 4), а
   * `mouse_move` шлёт `window`-слушатель выше, а не этот обработчик: он не увидел бы движение,
   * начавшееся ещё до входа курсора в холст.
   */
  function handlePointerMove(event: React.PointerEvent<HTMLCanvasElement>): void {
    if (!engine || isGameInputActiveRef.current) return;
    const drag = dragRef.current;
    if (drag === null || drag.pointerId !== event.pointerId) return;
    const deltaX = event.clientX - drag.startClientX;
    const deltaY = event.clientY - drag.startClientY;
    if (!drag.hasStartedDrag) {
      // «Редактор», требование 7: сдвиг дальше 4 пикселей начинает перенос.
      if (!hasCrossedDragThreshold(deltaX, deltaY)) return;
      drag.hasStartedDrag = true;
    }
    const position = computeDragPosition(drag.startPosition, [deltaX, deltaY], drag.cellSizePx, event.ctrlKey);
    if (position[0] === drag.lastPosition[0] && position[1] === drag.lastPosition[1]) return;
    drag.lastPosition = position;
    engine.move_object(drag.objectIndex, position[0], position[1]);
  }

  function endDrag(event: React.PointerEvent<HTMLCanvasElement>): void {
    if (isGameInputActiveRef.current) {
      // Только левая кнопка доходит до игры (требование 44) — нажатие правой/средней уже не дошло
      // до `mouse_down`, поэтому и её отпускание не должно звонить `mouse_up`.
      if (event.button === 0) engine?.mouse_up();
      return;
    }
    const drag = dragRef.current;
    if (drag === null || drag.pointerId !== event.pointerId) return;
    dragRef.current = null;
    event.currentTarget.releasePointerCapture(event.pointerId);
    if (drag.hasStartedDrag) onMoveObjectRef.current(drag.objectIndex, drag.lastPosition, drag.startPosition);
  }

  /** Браузер сам отменил перенос (например, второй палец на тачскрине) — объект возвращается на прежнее место, без действия. */
  function handlePointerCancel(event: React.PointerEvent<HTMLCanvasElement>): void {
    const drag = dragRef.current;
    if (drag === null || drag.pointerId !== event.pointerId) return;
    dragRef.current = null;
    event.currentTarget.releasePointerCapture(event.pointerId);
    if (drag.hasStartedDrag) engine?.move_object(drag.objectIndex, drag.startPosition[0], drag.startPosition[1]);
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLCanvasElement>): void {
    const drag = dragRef.current;
    if (event.key !== "Escape" || drag === null) return;
    dragRef.current = null;
    engine?.move_object(drag.objectIndex, drag.startPosition[0], drag.startPosition[1]);
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
          onPointerMove={handlePointerMove}
          onPointerUp={endDrag}
          onPointerCancel={handlePointerCancel}
          onKeyDown={handleKeyDown}
        />
      </div>
      {sceneSize !== null && (
        <span className="scene-view__size">
          Сцена {sceneSize.width} × {sceneSize.height}
        </span>
      )}
      {areHandlesAvailable && (
        <HandleModeToolbar
          mode={handleMode}
          brushKind={brushKind}
          areBrushesAvailable={areBrushesAvailable}
          brushSize={brushSize}
          brushStrength={brushStrength}
          water={terrainWater}
          mountainTool={{
            isSelected: selectedTool.isMountainTool,
            isEnabled: isMountainEnabled,
            fields: {
              stampNames: stampShapes.map((shape) => shape.name),
              stamp: mountainStamp?.name ?? "",
              width: mountainWidth,
              height: mountainHeight,
              onStampChange: setMountainStampName,
              onWidthChange: setMountainWidth,
              onHeightChange: setMountainHeight,
            },
            onSelect: () => setSelectedTool(selectMountainTool(handleMode)),
          }}
          paintTool={{
            isSelected: selectedTool.isPaintTool,
            isEnabled: isPaintEnabled,
            fields: {
              size: brushSize,
              strength: brushStrength,
              materialNames,
              material: paintMaterial ?? "",
              blockedMaterials,
              onSizeChange: brushFields.onSizeChange,
              onStrengthChange: brushFields.onStrengthChange,
              onMaterialChange: setPaintMaterialName,
            },
            onSelect: () => setSelectedTool(selectPaintTool(handleMode)),
          }}
          onChange={selectHandleMode}
          onBrushChange={(kind) => setSelectedTool(selectBrushTool(handleMode, kind))}
          onBrushSizeChange={brushFields.onSizeChange}
          onBrushStrengthChange={brushFields.onStrengthChange}
          onWaterChange={onWaterChange}
        />
      )}
    </div>
  );
}
