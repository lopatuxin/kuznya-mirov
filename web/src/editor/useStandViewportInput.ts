import { motion_turn_after } from "engine";
import { useEffect, useRef, type RefObject } from "react";
import { wheelDeltaPoints } from "../cameraInput";
import { effectiveBoneTurn, isMarked, motionsTextWithBoneTurn, type BoneTurn } from "./motionsFile";
import type { AnimationStand } from "./useAnimationStand";

/** Поправки пишутся с точностью до сотой градуса: разница меньше — кольцо не сдвинулось. */
const ROUNDING = 0.005;

type Gesture =
  | { kind: "rotate"; x: number; y: number }
  | { kind: "ring"; axis: number; bone: number; boneName: string; phase: number; turn: BoneTurn; original: BoneTurn };

export type StandViewportInput = {
  handlers: {
    onPointerDown: (event: React.PointerEvent<HTMLCanvasElement>) => void;
    onPointerMove: (event: React.PointerEvent<HTMLCanvasElement>) => void;
    onPointerUp: (event: React.PointerEvent<HTMLCanvasElement>) => void;
    onPointerCancel: (event: React.PointerEvent<HTMLCanvasElement>) => void;
    onContextMenu: (event: React.MouseEvent<HTMLCanvasElement>) => void;
  };
  /** Кольцо под указателем или в руке: 0, 1 или 2 — кадровый цикл рисует его толще. */
  getActiveRing: () => number | null;
  /** Камеру водили мышью: смена размера окна уже не возвращает её к виду. */
  hasMovedCamera: () => boolean;
  resetCameraMoved: () => void;
};

/**
 * Мышь над просмотром — «Анимация», требования 6 и 10: левая кнопка выбирает кость на модели или тянет кольцо её поворота,
 * правая вращает вид, колесо приближает. Точки считаются от холста движка, а не от холста поверх него.
 *
 * `show_motion` собирает стенд заново и забывает хват кольца, поэтому кольцо тянется шагами: каждый шаг считает угол от
 * прежней точки на уже пересобранном стенде и прибавляет его к повороту кости.
 */
export function useStandViewportInput(stand: AnimationStand, worldCanvasRef: RefObject<HTMLCanvasElement | null>, overlayRef: RefObject<HTMLCanvasElement | null>): StandViewportInput {
  const standRef = useRef(stand);
  standRef.current = stand;
  const gestureRef = useRef<Gesture | null>(null);
  const hoverRingRef = useRef<number | null>(null);
  const isCameraMovedRef = useRef(false);

  function pointOf(event: { clientX: number; clientY: number }): [number, number] | null {
    const bounds = worldCanvasRef.current?.getBoundingClientRect();
    return bounds === undefined ? null : [event.clientX - bounds.left, event.clientY - bounds.top];
  }

  useEffect(() => {
    const overlay = overlayRef.current;
    if (overlay === null) return;
    function handleWheel(event: WheelEvent): void {
      event.preventDefault();
      isCameraMovedRef.current = true;
      standRef.current.engine?.motion_zoom(wheelDeltaPoints(event.deltaY, event.deltaMode));
    }
    overlay.addEventListener("wheel", handleWheel, { passive: false });
    return () => overlay.removeEventListener("wheel", handleWheel);
  }, [overlayRef]);

  function beginRing(axis: number, bone: number, point: [number, number]): void {
    const { engine, info, corrections, isCyclic, playback } = standRef.current;
    const boneName = info?.bones[bone]?.name;
    if (engine === null || boneName === undefined) return;
    const correction = corrections[boneName];
    // Отметки зависят от доли: кольцо правит ту отметку, на которой движение стоит.
    if (isMarked(correction)) playback.setPlaying(false);
    const phase = playback.getState().phase;
    engine.motion_ring_grab(bone, axis, point[0], point[1]);
    standRef.current.setGestureActive(true);
    const original = effectiveBoneTurn(correction, phase, isCyclic);
    gestureRef.current = { kind: "ring", axis, bone, boneName, phase, turn: original, original };
    hoverRingRef.current = axis;
  }

  function dragRing(gesture: Extract<Gesture, { kind: "ring" }>, point: [number, number]): void {
    const engine = standRef.current.engine;
    const degrees = engine?.motion_ring_drag(point[0], point[1]) as number | undefined;
    if (engine === null || degrees === undefined || !Number.isFinite(degrees)) return;
    const turned = motion_turn_after(gesture.turn[0], gesture.turn[1], gesture.turn[2], gesture.axis, degrees) as number[];
    gesture.turn = [turned[0] as number, turned[1] as number, turned[2] as number];
    const turn = gesture.turn;
    standRef.current.previewNow((text, name) => motionsTextWithBoneTurn(text, name, gesture.boneName, gesture.phase, turn));
    engine.motion_ring_grab(gesture.bone, gesture.axis, point[0], point[1]);
  }

  function release(event: React.PointerEvent<HTMLCanvasElement>): void {
    const gesture = gestureRef.current;
    gestureRef.current = null;
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (gesture?.kind !== "ring") return;
    const current = standRef.current;
    current.engine?.motion_ring_release();
    current.setGestureActive(false);
    const turn = gesture.turn;
    const isUnchanged = turn.every((angle, axis) => Math.abs(angle - (gesture.original[axis] as number)) < ROUNDING);
    // Кольцо схвачено и отпущено на месте: файл не пишется, а стенд, что мог дрогнуть при шаге, возвращается к нему.
    current.commit(isUnchanged ? () => null : (text, name) => motionsTextWithBoneTurn(text, name, gesture.boneName, gesture.phase, turn));
  }

  const handlers: StandViewportInput["handlers"] = {
    onPointerDown: (event) => {
      const { engine, selectedBone, info } = standRef.current;
      const point = pointOf(event);
      if (engine === null || info === null || point === null) return;
      if (event.button === 2) {
        event.currentTarget.setPointerCapture(event.pointerId);
        gestureRef.current = { kind: "rotate", x: event.clientX, y: event.clientY };
        return;
      }
      if (event.button !== 0) return;
      if (selectedBone !== null) {
        const axis = engine.motion_ring_at(selectedBone, point[0], point[1]) as number | undefined;
        if (axis !== undefined) {
          event.currentTarget.setPointerCapture(event.pointerId);
          beginRing(axis, selectedBone, point);
          return;
        }
      }
      standRef.current.selectBone((engine.motion_bone_at(point[0], point[1]) as number | undefined) ?? null);
    },
    onPointerMove: (event) => {
      const { engine, selectedBone } = standRef.current;
      const gesture = gestureRef.current;
      const point = pointOf(event);
      if (engine === null || point === null) return;
      if (gesture?.kind === "rotate") {
        isCameraMovedRef.current = true;
        engine.motion_turn(event.clientX - gesture.x, event.clientY - gesture.y);
        gesture.x = event.clientX;
        gesture.y = event.clientY;
      } else if (gesture?.kind === "ring") {
        dragRing(gesture, point);
      } else {
        hoverRingRef.current = selectedBone === null ? null : ((engine.motion_ring_at(selectedBone, point[0], point[1]) as number | undefined) ?? null);
      }
    },
    onPointerUp: release,
    onPointerCancel: release,
    // Правая кнопка вращает вид — меню браузера над просмотром не нужно.
    onContextMenu: (event) => event.preventDefault(),
  };

  return {
    handlers,
    getActiveRing: () => hoverRingRef.current,
    hasMovedCamera: () => isCameraMovedRef.current,
    resetCameraMoved: () => {
      isCameraMovedRef.current = false;
    },
  };
}
