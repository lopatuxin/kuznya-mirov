import { useEffect, useRef, type RefObject } from "react";
import { createSpaceSceneController, type PointerInput, type SpaceSceneContext, type SpaceSceneController } from "./spaceSceneController";

type UseSpaceSceneInputParams = {
  overlayCanvasRef: RefObject<HTMLCanvasElement | null>;
  /** Трёхмерная сцена с движком: только тогда события холста идут в контроллер. */
  context: SpaceSceneContext | null;
  /** Ссылка меняется, когда объекты изменились под жестом извне — жест бросается («Редактор», крайние случаи). */
  objectsVersion: unknown;
};

const MIDDLE_BUTTON = 1;

/** Захват доносит отпускание до холста, даже если кнопку отпустили за ним; у указателя, которого уже нет, захвата нет — и жесту он не нужен. */
function capturePointer(canvas: HTMLCanvasElement, pointerId: number): void {
  try {
    canvas.setPointerCapture(pointerId);
  } catch {
    // нет такого активного указателя
  }
}

/**
 * Связывает события холста с контроллером трёхмерной сцены: нажатие, движение и отпускание
 * указателя, колесо (`passive: false` — иначе страницу листало бы) и клавиши. `mousedown` средней
 * кнопки гасится вместе с `pointerdown`: без этого Windows включает автопрокрутку. Возвращает тот же
 * контроллер на всю жизнь холста — кадровый цикл рисует им рамку и ручки.
 */
export function useSpaceSceneInput({ overlayCanvasRef, context, objectsVersion }: UseSpaceSceneInputParams): SpaceSceneController {
  const contextRef = useRef(context);
  contextRef.current = context;
  const controllerRef = useRef<SpaceSceneController | null>(null);
  if (controllerRef.current === null) controllerRef.current = createSpaceSceneController(() => contextRef.current as SpaceSceneContext);
  const controller = controllerRef.current;
  const isEnabled = context !== null;
  const isInputLocked = context?.isInputLocked ?? false;

  useEffect(() => {
    controller.abandonGesture();
  }, [controller, objectsVersion, isInputLocked]);

  useEffect(() => {
    const canvas = overlayCanvasRef.current;
    if (!isEnabled || !canvas) return;
    const activeCanvas = canvas;

    function toInput(event: PointerEvent): PointerInput {
      const bounds = activeCanvas.getBoundingClientRect();
      return {
        pointerId: event.pointerId,
        button: event.button,
        buttons: event.buttons,
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
        ctrlKey: event.ctrlKey,
        shiftKey: event.shiftKey,
      };
    }

    function handleMouseDown(event: MouseEvent): void {
      if (event.button === MIDDLE_BUTTON) event.preventDefault();
    }

    function handlePointerDown(event: PointerEvent): void {
      if (event.button === MIDDLE_BUTTON) event.preventDefault();
      if (event.button !== 0 && event.button !== MIDDLE_BUTTON) return;
      if (contextRef.current?.isInputLocked) return;
      // Открытое поле свойства записывается в прежний объект до смены выбора — как в плоской сцене.
      if (document.activeElement instanceof HTMLElement && document.activeElement !== activeCanvas) document.activeElement.blur();
      // Клавиши `W`, `E`, `R`, `F` и `Esc` работают, когда фокус на сцене.
      activeCanvas.focus({ preventScroll: true });
      if (controller.pointerDown(toInput(event))) capturePointer(activeCanvas, event.pointerId);
    }

    function handlePointerMove(event: PointerEvent): void {
      controller.pointerMove(toInput(event));
    }

    function handlePointerUp(event: PointerEvent): void {
      controller.pointerUp(toInput(event));
    }

    function handlePointerCancel(event: PointerEvent): void {
      controller.pointerCancel(toInput(event));
    }

    function handleWheel(event: WheelEvent): void {
      if (controller.wheel({ deltaY: event.deltaY, deltaMode: event.deltaMode })) event.preventDefault();
    }

    function handleKeyDown(event: KeyboardEvent): void {
      if (controller.keyDown(event)) event.preventDefault();
    }

    activeCanvas.addEventListener("mousedown", handleMouseDown);
    activeCanvas.addEventListener("pointerdown", handlePointerDown);
    activeCanvas.addEventListener("pointermove", handlePointerMove);
    activeCanvas.addEventListener("pointerup", handlePointerUp);
    activeCanvas.addEventListener("pointercancel", handlePointerCancel);
    activeCanvas.addEventListener("pointerleave", controller.pointerLeave);
    activeCanvas.addEventListener("wheel", handleWheel, { passive: false });
    activeCanvas.addEventListener("keydown", handleKeyDown);
    return () => {
      activeCanvas.removeEventListener("mousedown", handleMouseDown);
      activeCanvas.removeEventListener("pointerdown", handlePointerDown);
      activeCanvas.removeEventListener("pointermove", handlePointerMove);
      activeCanvas.removeEventListener("pointerup", handlePointerUp);
      activeCanvas.removeEventListener("pointercancel", handlePointerCancel);
      activeCanvas.removeEventListener("pointerleave", controller.pointerLeave);
      activeCanvas.removeEventListener("wheel", handleWheel);
      activeCanvas.removeEventListener("keydown", handleKeyDown);
      controller.abandonGesture();
    };
  }, [overlayCanvasRef, controller, isEnabled]);

  return controller;
}
