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

/** Поля, у которых нажатие не флажок и не кнопка, а ввод: текст, число, выбор из списка. */
const NON_TYPING_INPUTS = new Set(["checkbox", "radio", "color", "range", "button"]);

/** В элемент печатают — клавиши сцены ему не мешают: Esc отменяет ввод, W и E пишут буквы. */
function isTypingTarget(target: EventTarget | null): boolean {
  if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  if (target instanceof HTMLInputElement) return !NON_TYPING_INPUTS.has(target.type);
  return target instanceof HTMLElement && target.isContentEditable;
}

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
 * указателя, колесо (`passive: false` — иначе страницу листало бы) и клавиши (нажатие и отпускание — Shift мазка). `mousedown` средней
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
      // Открытое поле свойства записывается в прежний объект до смены выбора — как в плоской сцене.
      if (document.activeElement instanceof HTMLElement && document.activeElement !== activeCanvas) document.activeElement.blur();
      // Фокус — на сцену: поле, из которого щёлкнули, перестаёт ловить клавиши, а в партии они доходят до игры.
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
      if (controller.wheel({ deltaY: event.deltaY, deltaMode: event.deltaMode, ctrlKey: event.ctrlKey })) event.preventDefault();
    }

    function handleKeyDown(event: KeyboardEvent): void {
      // Клавишу уже взял себе элемент страницы (Esc в списке объектов) или в элемент печатают — сцене она не достаётся.
      if (event.defaultPrevented || isTypingTarget(event.target)) return;
      if (controller.keyDown(event)) event.preventDefault();
    }

    function handleKeyUp(event: KeyboardEvent): void {
      controller.keyUp(event);
    }

    activeCanvas.addEventListener("mousedown", handleMouseDown);
    activeCanvas.addEventListener("pointerdown", handlePointerDown);
    activeCanvas.addEventListener("pointermove", handlePointerMove);
    activeCanvas.addEventListener("pointerup", handlePointerUp);
    activeCanvas.addEventListener("pointercancel", handlePointerCancel);
    activeCanvas.addEventListener("pointerleave", controller.pointerLeave);
    activeCanvas.addEventListener("wheel", handleWheel, { passive: false });
    // Клавиши сцены (Esc, `W`, `E`, `R`, `F`, Shift мазка) — на всей странице, а не только на холсте: инструмент включают
    // кнопкой в полосе, и пока по сцене не щёлкнули, фокус не на ней.
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    return () => {
      activeCanvas.removeEventListener("mousedown", handleMouseDown);
      activeCanvas.removeEventListener("pointerdown", handlePointerDown);
      activeCanvas.removeEventListener("pointermove", handlePointerMove);
      activeCanvas.removeEventListener("pointerup", handlePointerUp);
      activeCanvas.removeEventListener("pointercancel", handlePointerCancel);
      activeCanvas.removeEventListener("pointerleave", controller.pointerLeave);
      activeCanvas.removeEventListener("wheel", handleWheel);
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
      controller.abandonGesture();
    };
  }, [overlayCanvasRef, controller, isEnabled]);

  return controller;
}
