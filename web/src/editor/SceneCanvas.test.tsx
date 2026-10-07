// @vitest-environment jsdom
import type { Engine } from "engine";
import { cleanup, createEvent, fireEvent, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createEditorCameraStore, createFlatCameraStore } from "./editorCamera";
import { IMAGE_DRAG_TYPE, PARTICLES_DRAG_TYPE } from "./imageDrag";
import { MAX_FRAME_SECONDS } from "./frameClock";
import { NO_MASKS } from "./maskBytes";
import { SceneCanvas } from "./SceneCanvas";

type SceneCanvasProps = Parameters<typeof SceneCanvas>[0];

/** Движок, который на любой вызов отвечает пустотой, а `draw` записывает: кадровому циклу от него нужен только `draw`. */
function createDrawRecordingEngine(): { engine: Engine; draw: ReturnType<typeof vi.fn> } {
  const draw = vi.fn();
  const engine = new Proxy({ draw }, { get: (target, name) => (name in target ? target[name as keyof typeof target] : () => undefined) }) as unknown as Engine;
  return { engine, draw };
}

function sceneCanvasProps(engine: Engine): SceneCanvasProps {
  const noop = (): void => {};
  return {
    canvasRef: { current: null },
    engine,
    sceneSize: { width: 100, height: 50 },
    objectsVersion: [],
    canEditScene: true,
    isSceneShown: true,
    isGameInputActive: false,
    isThreeDimensionalScene: false,
    isEditorCameraActive: true,
    editorCameraStore: createEditorCameraStore(),
    flatCameraStore: createFlatCameraStore(),
    getObjectProperties: () => null,
    selectedIndex: null,
    selectedLabel: null,
    onSelect: noop,
    onCommitPlacement: noop,
    onDropImage: noop,
    onDropParticles: noop,
    terrainWater: null,
    onCommitTerrain: noop,
    onWaterChange: noop,
    wind: [0, 0],
    onWindChange: () => undefined,
    onStrokeActiveChange: noop,
    brushFields: { size: 4, strength: 50, onSizeChange: noop, onStrengthChange: noop },
    imprints: [],
    stampShapes: [],
    stampPreviews: new Map(),
    selectedImprintIndex: null,
    onSelectImprint: noop,
    onPlaceImprint: noop,
    onCommitImprint: noop,
    materialNames: [],
    terrainCovers: null,
    terrainMasks: NO_MASKS,
    hasTerrainFile: false,
    terrainTintPath: null,
    onCommitPaint: noop,
    onRestorePaint: noop,
    toolbarSlot: null,
    clearPreviewRef: { current: noop },
  };
}

describe("SceneCanvas: кадровый цикл", () => {
  let pendingFrame: FrameRequestCallback | null;

  beforeEach(() => {
    pendingFrame = null;
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      pendingFrame = callback;
      return 1;
    });
    vi.stubGlobal("cancelAnimationFrame", () => {
      pendingFrame = null;
    });
    vi.stubGlobal("ResizeObserver", class { observe(): void {} disconnect(): void {} });
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  function runFrame(time: number): void {
    const frame = pendingFrame;
    pendingFrame = null;
    frame?.(time);
  }

  it("передаёт в draw секунды с прошлого кадра: первый — 0, дальше разница времён", () => {
    const { engine, draw } = createDrawRecordingEngine();
    render(<SceneCanvas {...sceneCanvasProps(engine)} />);

    runFrame(1000);
    runFrame(1016);
    runFrame(1066);

    expect(draw.mock.calls.map(([seconds]) => seconds)).toEqual([0, 0.016, 0.05]);
  });

  it("кадр после долгой остановки вкладки двигает часы не больше чем на MAX_FRAME_SECONDS", () => {
    const { engine, draw } = createDrawRecordingEngine();
    render(<SceneCanvas {...sceneCanvasProps(engine)} />);

    runFrame(1000);
    runFrame(61000);

    expect(draw.mock.calls.map(([seconds]) => seconds)).toEqual([0, MAX_FRAME_SECONDS]);
  });

  it("перерисовка окна часы не сбрасывает: следующий кадр считает от прежнего", () => {
    const { engine, draw } = createDrawRecordingEngine();
    const { rerender } = render(<SceneCanvas {...sceneCanvasProps(engine)} />);
    runFrame(1000);

    rerender(<SceneCanvas {...sceneCanvasProps(engine)} isEditorCameraActive={false} />);
    runFrame(1030);

    expect(draw.mock.calls.map(([seconds]) => seconds)).toEqual([0, 0.03]);
  });

  it("часы начинаются заново с новым движком: первый кадр нового движка — 0", () => {
    const first = createDrawRecordingEngine();
    const second = createDrawRecordingEngine();
    const { rerender } = render(<SceneCanvas {...sceneCanvasProps(first.engine)} />);
    runFrame(1000);

    rerender(<SceneCanvas {...sceneCanvasProps(second.engine)} />);
    runFrame(1500);

    expect(second.draw.mock.calls.map(([seconds]) => seconds)).toEqual([0]);
  });
});

describe("SceneCanvas: вид частиц, отпущенный на сцену", () => {
  beforeEach(() => {
    vi.stubGlobal("requestAnimationFrame", () => 1);
    vi.stubGlobal("cancelAnimationFrame", () => {});
    vi.stubGlobal("ResizeObserver", class { observe(): void {} disconnect(): void {} });
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  function dropOnScene(props: Partial<SceneCanvasProps>, types: string[], data: Record<string, string>): void {
    const { engine } = createDrawRecordingEngine();
    const { container } = render(<SceneCanvas {...sceneCanvasProps(engine)} {...props} />);
    const overlay = container.querySelector(".scene-view__overlay") as HTMLElement;
    // jsdom не знает DragEvent: точку отпускания кладём на событие сами.
    const drop = createEvent.drop(overlay, { dataTransfer: { types, getData: (type: string) => data[type] ?? "" } });
    Object.defineProperties(drop, { clientX: { value: 30 }, clientY: { value: 20 } });
    fireEvent(overlay, drop);
  }

  it("имя вида и точка холста уходят onDropParticles, картинка — не его", () => {
    const onDropParticles = vi.fn();
    const onDropImage = vi.fn();

    dropOnScene({ onDropParticles, onDropImage }, [PARTICLES_DRAG_TYPE], { [PARTICLES_DRAG_TYPE]: "дым" });

    expect(onDropParticles).toHaveBeenCalledWith("дым", 30, 20);
    expect(onDropImage).not.toHaveBeenCalled();
  });

  it("картинка уходит onDropImage, а не onDropParticles", () => {
    const onDropParticles = vi.fn();
    const onDropImage = vi.fn();

    dropOnScene({ onDropParticles, onDropImage }, [IMAGE_DRAG_TYPE], { [IMAGE_DRAG_TYPE]: "izba" });

    expect(onDropImage).toHaveBeenCalledWith("izba", 30, 20);
    expect(onDropParticles).not.toHaveBeenCalled();
  });

  it("сцену нельзя править — вид не принимается", () => {
    const onDropParticles = vi.fn();

    dropOnScene({ onDropParticles, canEditScene: false }, [PARTICLES_DRAG_TYPE], { [PARTICLES_DRAG_TYPE]: "дым" });

    expect(onDropParticles).not.toHaveBeenCalled();
  });

  it("в трёхмерной сцене вид не принимается", () => {
    const onDropParticles = vi.fn();

    dropOnScene({ onDropParticles, isThreeDimensionalScene: true }, [PARTICLES_DRAG_TYPE], { [PARTICLES_DRAG_TYPE]: "дым" });

    expect(onDropParticles).not.toHaveBeenCalled();
  });
});
