import { useEffect, useRef, type RefObject } from "react";
import { createFlatSceneController, type FlatSceneContext, type FlatSceneController } from "./flatSceneController";
import { capturePointer, isTypingTarget } from "./sceneInputDom";
import type { PointerInput } from "./spaceSceneController";

type UseFlatSceneInputParams = {
  overlayCanvasRef: RefObject<HTMLCanvasElement | null>;
  /** Плоская сцена с движком: только тогда события холста идут в контроллер. */
  context: FlatSceneContext | null;
  /** Ссылка меняется, когда объекты изменились под жестом извне — жест бросается («Редактор», крайние случаи). */
  objectsVersion: unknown;
};

const MIDDLE_BUTTON = 1;

/**
 * Связывает события холста с контроллером плоской сцены, как `useSpaceSceneInput` — трёхмерной: нажатие, движение и
 * отпускание указателя, колесо (`passive: false` — иначе страницу листало бы или Ctrl+колесо масштабировало её) и
 * клавиши на всей странице. `mousedown` средней кнопки гасится вместе с `pointerdown`: без этого Windows включает
 * автопрокрутку. Возвращает тот же контроллер на всю жизнь холста — кадровый цикл рисует им границу, рамку и ручки.
 */
export function useFlatSceneInput({ overlayCanvasRef, context, objectsVersion }: UseFlatSceneInputParams): FlatSceneController {
  const contextRef = useRef(context);
  contextRef.current = context;
  const controllerRef = useRef<FlatSceneController | null>(null);
  if (controllerRef.current === null) controllerRef.current = createFlatSceneController(() => contextRef.current as FlatSceneContext);
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
        timeStamp: event.timeStamp,
      };
    }

    function handleMouseDown(event: MouseEvent): void {
      if (event.button === MIDDLE_BUTTON) event.preventDefault();
    }

    function handlePointerDown(event: PointerEvent): void {
      if (event.button === MIDDLE_BUTTON) event.preventDefault();
      if (event.button !== 0 && event.button !== MIDDLE_BUTTON) return;
      if (contextRef.current?.isInputLocked) return;
      // Открытое поле свойства записывается в прежний объект до смены выбора.
      if (document.activeElement instanceof HTMLElement && document.activeElement !== activeCanvas) document.activeElement.blur();
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
      const bounds = activeCanvas.getBoundingClientRect();
      const input = { deltaY: event.deltaY, deltaMode: event.deltaMode, ctrlKey: event.ctrlKey, x: event.clientX - bounds.left, y: event.clientY - bounds.top };
      if (controller.wheel(input)) event.preventDefault();
    }

    function handleKeyDown(event: KeyboardEvent): void {
      // Клавишу уже взял себе элемент страницы (Esc в списке объектов) или в элемент печатают — сцене она не достаётся.
      if (event.defaultPrevented || isTypingTarget(event.target)) return;
      if (controller.keyDown(event)) event.preventDefault();
    }

    activeCanvas.addEventListener("mousedown", handleMouseDown);
    activeCanvas.addEventListener("pointerdown", handlePointerDown);
    activeCanvas.addEventListener("pointermove", handlePointerMove);
    activeCanvas.addEventListener("pointerup", handlePointerUp);
    activeCanvas.addEventListener("pointercancel", handlePointerCancel);
    activeCanvas.addEventListener("pointerleave", controller.pointerLeave);
    activeCanvas.addEventListener("wheel", handleWheel, { passive: false });
    // Клавиши сцены (Esc, `W`, `R`, `F`) — на всей странице, а не только на холсте: инструмент включают
    // кнопкой в полосе, и пока по сцене не щёлкнули, фокус не на ней.
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      activeCanvas.removeEventListener("mousedown", handleMouseDown);
      activeCanvas.removeEventListener("pointerdown", handlePointerDown);
      activeCanvas.removeEventListener("pointermove", handlePointerMove);
      activeCanvas.removeEventListener("pointerup", handlePointerUp);
      activeCanvas.removeEventListener("pointercancel", handlePointerCancel);
      activeCanvas.removeEventListener("pointerleave", controller.pointerLeave);
      activeCanvas.removeEventListener("wheel", handleWheel);
      window.removeEventListener("keydown", handleKeyDown);
      controller.abandonGesture();
    };
  }, [overlayCanvasRef, controller, isEnabled]);

  return controller;
}
