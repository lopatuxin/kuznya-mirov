import { useEffect, useRef, type RefObject } from "react";
import { secondsSinceLastFrame } from "./frameClock";
import { drawStandOverlay, type StandRings } from "./standOverlayDrawing";
import { StandTimeline } from "./StandTimeline";
import { useStandViewportInput } from "./useStandViewportInput";
import type { AnimationStand, StandPose, StandView } from "./useAnimationStand";

const VIEW_BUTTONS: readonly { view: StandView; label: string }[] = [
  { view: "side", label: "Сбоку" },
  { view: "front", label: "Спереди" },
  { view: "game", label: "Камера игры" },
];

function placeholderText(hasNoMotions: boolean, hasErrors: boolean): string {
  if (hasNoMotions) return "Создайте движение слева — здесь модель побежит на месте";
  return hasErrors ? "Просмотр появится, когда движение соберётся без ошибок" : "Движение собирается…";
}

type StandViewportProps = {
  stand: AnimationStand;
  /** Холст движка: по нему считаются точки мыши, и на нём модель стоит на ровной земле. */
  worldCanvasRef: RefObject<HTMLCanvasElement | null>;
  boneName: string | undefined;
  /** Ошибки `motions.json` из проверки проекта, по строке на ошибку. */
  projectErrorLines: readonly string[];
  isEditable: boolean;
};

/**
 * Середина экрана «Анимация» — «Редактор», «Анимация», требования 6–7, 10–12: кнопки видов над просмотром,
 * текст ошибки `motions.json`, рисование поверх модели (суставы, кость, кольца поворота) и шкала под ним. Модель на холсте
 * рисует движок, а что с ней можно сделать мышью, решает `useStandViewportInput`.
 */
export function StandViewport({ stand, worldCanvasRef, boneName, projectErrorLines, isEditable }: StandViewportProps): React.JSX.Element {
  const overlayRef = useRef<HTMLCanvasElement>(null);
  const input = useStandViewportInput(stand, worldCanvasRef, overlayRef);
  const latestRef = useRef({ stand, input });
  latestRef.current = { stand, input };
  const { engine, info } = stand;
  const errorLines = [...projectErrorLines, ...(stand.error === null ? [] : stand.error.split("\n"))];

  // Кадр страницы: движение идёт вперёд, движок ставит позу, поверх неё рисуются суставы и кольца.
  useEffect(() => {
    const overlay = overlayRef.current;
    const world = worldCanvasRef.current;
    if (engine === null || overlay === null || world === null || info === null) return;
    const context = overlay.getContext("2d");
    if (context === null) return;
    const activeEngine = engine;
    const overlayCanvas = overlay;
    const worldCanvas = world;
    const overlayContext = context;
    let frameHandle = 0;
    let previousTime: number | null = null;
    function frame(time: number): void {
      const { stand: current, input: currentInput } = latestRef.current;
      const shown = current.info;
      if (shown === null) return;
      current.playback.advance(secondsSinceLastFrame(previousTime, time), shown.length);
      previousTime = time;
      const pose = activeEngine.motion_pose(current.playback.getState().phase) as StandPose | null;
      const pixelRatio = window.devicePixelRatio || 1;
      const overlayBounds = overlayCanvas.getBoundingClientRect();
      const worldBounds = worldCanvas.getBoundingClientRect();
      const width = Math.round(overlayBounds.width * pixelRatio);
      const height = Math.round(overlayBounds.height * pixelRatio);
      if (overlayCanvas.width !== width || overlayCanvas.height !== height) {
        overlayCanvas.width = width;
        overlayCanvas.height = height;
      }
      overlayContext.clearRect(0, 0, width, height);
      if (pose !== null) {
        const bone = current.selectedBone;
        drawStandOverlay(overlayContext, {
          pose,
          bones: shown.bones,
          selectedBone: bone,
          rings: bone === null ? undefined : (activeEngine.motion_rings(bone) as StandRings | undefined),
          activeRing: currentInput.getActiveRing(),
          offset: [worldBounds.left - overlayBounds.left, worldBounds.top - overlayBounds.top],
          pixelRatio,
        });
      }
      frameHandle = requestAnimationFrame(frame);
    }
    frameHandle = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(frameHandle);
  }, [engine, info !== null, worldCanvasRef]);

  // Холст движка подгоняется под экран уже после того, как стенд собран: камера, которую не водили, встаёт на вид заново.
  useEffect(() => {
    const world = worldCanvasRef.current;
    if (engine === null || world === null || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      const { stand: current, input: currentInput } = latestRef.current;
      if (current.info !== null && !currentInput.hasMovedCamera()) engine.motion_view(current.view);
    });
    observer.observe(world);
    return () => observer.disconnect();
  }, [engine, worldCanvasRef]);

  function selectView(view: StandView): void {
    input.resetCameraMoved();
    stand.setView(view);
  }

  return (
    <div className="stand-viewport">
      <div className="stand-viewport__toolbar">
        <div className="scene-tools__group" role="group" aria-label="Вид">
          {VIEW_BUTTONS.map(({ view, label }) => (
            <button key={view} type="button" className={stand.view === view ? "scene-tools__button scene-tools__button--active" : "scene-tools__button"} aria-pressed={stand.view === view} onClick={() => selectView(view)}>
              {label}
            </button>
          ))}
        </div>
      </div>
      {errorLines.length > 0 && (
        <div className="editor-alert editor-alert--error stand-viewport__error" role="alert">
          <span>
            {errorLines.map((line) => (
              <span key={line} className="stand-viewport__error-line">
                {line}
              </span>
            ))}
          </span>
        </div>
      )}
      {info === null && (
        <div className="stand-viewport__placeholder">
          <span>{placeholderText(stand.motionNames.length === 0, errorLines.length > 0)}</span>
        </div>
      )}
      <canvas ref={overlayRef} className="stand-viewport__overlay" tabIndex={-1} {...input.handlers} />
      <div className="stand-viewport__timeline">
        <StandTimeline stand={stand} boneName={boneName} isEditable={isEditable} />
      </div>
    </div>
  );
}
