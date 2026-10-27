import type { Engine } from "engine";
import { useEffect, useMemo, useRef, useState } from "react";
import { readBoneCorrections, readMotionNames, readMotionSettings, type BoneCorrection, type MotionSettings } from "./motionsFile";
import { createStandPlayback, type StandPlayback } from "./standPlayback";

export type StandView = "side" | "front" | "game";
export type StandBone = { name: string; parent: number };
/** Что движок сказал о собранном стенде: кости скелета, длина движения в секундах с учётом `speed` и число кадров в нём. */
export type StandInfo = { bones: StandBone[]; length: number; frames: number };
/** Доли, где ступня `foot` (0 или 1) стоит на земле и скользит. */
export type StandSlip = { foot: number; from: number; to: number };
export type StandEndGap = { degrees: number; bone: string };
export type StandAnalysis = { slips: StandSlip[]; endGap: StandEndGap | null };
/** Ответ `motion_pose`: точки суставов на холсте по костям `show_motion`. */
export type StandPose = {
  phase: number;
  points: (readonly [number, number] | null)[];
  slips: StandSlip[];
  end_gap: StandEndGap | null;
};

type ShowMotionResult = ({ ok: true } & StandInfo) | { ok: false; error: string };

/** Правка текста `motions.json` для движения `name`; `null` — правки нет. */
export type MotionsChange = (motionsText: string | null, name: string) => string | null;

const NO_ANALYSIS: StandAnalysis = { slips: [], endGap: null };

/** Прежнее значение, если новое то же: окно не перерисовывается из-за каждого пересбора стенда. */
function keepIfSame<T>(previous: T, next: T): T {
  return JSON.stringify(previous) === JSON.stringify(next) ? previous : next;
}

type AnimationStandOptions = {
  engine: Engine | null;
  /** Экран «Анимация» открыт: движок показывает стенд вместо сцены. */
  isOpen: boolean;
  /** Проект загружен без ошибок: у движка есть игра, из которой собирается стенд. Ошибки проекта показывает сам проект, а просмотр остаётся на последней верной позе. */
  isProjectLoaded: boolean;
  motionsText: string | null;
  editMotions: (build: (motionsText: string | null) => string | null) => void;
  createMotion: (name: string, model: string, clip: string) => void;
  /** Жест ползунка, кольца или засечки начался (`true`) и кончился (`false`): пока он идёт, перезагрузка файлов проекта ждёт. */
  setReloadGateOpen: (isOpen: boolean) => void;
  /** Меняется с каждой загрузкой проекта: стенд, что не собрался из-за ошибок проекта, собирается заново, когда они исправлены. */
  loadVersion: unknown;
};

export type AnimationStand = {
  engine: Engine | null;
  playback: StandPlayback;
  motionNames: string[];
  selectedName: string | null;
  selectMotion: (name: string) => void;
  createMotion: (name: string, model: string, clip: string) => void;
  settings: MotionSettings | null;
  corrections: Record<string, BoneCorrection>;
  /** Движение повторяется: поправки идут по кругу. */
  isCyclic: boolean;
  view: StandView;
  setView: (view: StandView) => void;
  /** Стенд собран; `null` — движения нет или движок отказал. */
  info: StandInfo | null;
  /** Текст ошибки проверки `motions.json` или причина, по которой стенд не собран. */
  error: string | null;
  analysis: StandAnalysis;
  selectedBone: number | null;
  selectBone: (bone: number | null) => void;
  /** Показывает правку на стенде сразу, без записи; движение за одним кадром страницы сливается в один показ. */
  preview: (change: MotionsChange) => void;
  /** Показывает правку на стенде тут же — для кольца и засечки, чей жест опирается на собранный стенд. */
  previewNow: (change: MotionsChange) => void;
  /** Пишет правку в `motions.json` одним шагом отмены; правки нет — стенд возвращается к тексту файла. */
  commit: (change: MotionsChange) => void;
  setGestureActive: (isActive: boolean) => void;
};

/**
 * Состояние экрана «Анимация» — «Редактор», «Анимация»: какое движение выбрано, вид камеры, выбранная кость и
 * то, что движок ответил о стенде. Стенд собирается заново из показанного текста `motions.json` при каждой правке, камера у
 * него остаётся прежней.
 */
export function useAnimationStand({ engine, isOpen, isProjectLoaded, motionsText, editMotions, createMotion, setReloadGateOpen, loadVersion }: AnimationStandOptions): AnimationStand {
  const [playback] = useState(createStandPlayback);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [pendingName, setPendingName] = useState<string | null>(null);
  const [view, setViewState] = useState<StandView>("game");
  const [info, setInfo] = useState<StandInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [analysis, setAnalysis] = useState<StandAnalysis>(NO_ANALYSIS);
  const [selectedBone, setSelectedBone] = useState<number | null>(null);
  const motionNames = useMemo(() => readMotionNames(motionsText), [motionsText]);
  const settings = useMemo(() => (selectedName === null ? null : readMotionSettings(motionsText, selectedName)), [motionsText, selectedName]);
  const corrections = useMemo(() => (selectedName === null ? {} : readBoneCorrections(motionsText, selectedName)), [motionsText, selectedName]);

  const latest = { engine, isProjectLoaded, motionsText, selectedName, view, editMotions };
  const latestRef = useRef(latest);
  latestRef.current = latest;
  const pendingPreviewRef = useRef<string | null>(null);
  const previewFrameRef = useRef(0);

  function rebuild(text: string): void {
    const { engine: activeEngine, isProjectLoaded: isLoaded, selectedName: name, view: shownView } = latestRef.current;
    if (activeEngine === null || name === null || !isLoaded) return;
    const result = activeEngine.show_motion(text, name, shownView) as ShowMotionResult;
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setError(null);
    const shown: StandInfo = { bones: result.bones, length: result.length, frames: result.frames };
    setInfo((previous) => (previous === null ? shown : keepIfSame(previous, shown)));
    // Скольжение ступней и расхождение концов от доли не зависят: их отдаёт любая поза.
    const pose = activeEngine.motion_pose(playback.getState().phase) as StandPose | null;
    const next: StandAnalysis = pose === null ? NO_ANALYSIS : { slips: pose.slips, endGap: pose.end_gap };
    setAnalysis((previous) => keepIfSame(previous, next));
  }

  function cancelPreview(): void {
    cancelAnimationFrame(previewFrameRef.current);
    previewFrameRef.current = 0;
    pendingPreviewRef.current = null;
  }

  function changedText(change: MotionsChange): string | null {
    const { motionsText: text, selectedName: name } = latestRef.current;
    return name === null ? null : change(text, name);
  }

  function previewNow(change: MotionsChange): void {
    cancelPreview();
    const text = changedText(change);
    if (text !== null) rebuild(text);
  }

  function preview(change: MotionsChange): void {
    const text = changedText(change);
    if (text === null) return;
    pendingPreviewRef.current = text;
    if (previewFrameRef.current !== 0) return;
    previewFrameRef.current = requestAnimationFrame(() => {
      const pending = pendingPreviewRef.current;
      previewFrameRef.current = 0;
      pendingPreviewRef.current = null;
      if (pending !== null) rebuild(pending);
    });
  }

  function commit(change: MotionsChange): void {
    cancelPreview();
    const { motionsText: text, selectedName: name } = latestRef.current;
    if (name === null) return;
    if (change(text, name) === null) {
      if (text !== null) rebuild(text);
      return;
    }
    latestRef.current.editMotions((current) => change(current, name));
  }

  // Движения пока нет в файле или выбранное пропало (отмена, правка файла снаружи) — выбрано первое.
  useEffect(() => {
    if (pendingName !== null) {
      if (!motionNames.includes(pendingName)) return;
      setSelectedName(pendingName);
      setPendingName(null);
      return;
    }
    if (selectedName === null || !motionNames.includes(selectedName)) setSelectedName(motionNames[0] ?? null);
  }, [motionNames, pendingName, selectedName]);

  // Другое движение — другой скелет: выбранная кость и показанное о стенде прежние не годятся.
  useEffect(() => {
    setSelectedBone(null);
    setError(null);
    setInfo(null);
    setAnalysis(NO_ANALYSIS);
  }, [selectedName, isOpen]);

  useEffect(() => {
    if (!isOpen || engine === null || selectedName === null || motionsText === null) return;
    rebuild(motionsText);
    // `rebuild` берёт всё нужное из `latestRef`.
  }, [engine, isOpen, isProjectLoaded, selectedName, motionsText]);

  // Движок отказал, потому что проект не загружался: когда загрузка прошла, стенд собирается из того же текста снова.
  useEffect(() => {
    if (isOpen && error !== null && motionsText !== null) rebuild(motionsText);
  }, [loadVersion]);

  // Закрытый экран возвращает холсту сцену, а отложенный показ не должен дойти до закрытого стенда.
  useEffect(() => {
    if (!isOpen || engine === null) return;
    return () => {
      cancelPreview();
      engine.hide_motion();
    };
  }, [engine, isOpen]);

  function setView(next: StandView): void {
    setViewState(next);
    latestRef.current.engine?.motion_view(next);
  }

  return {
    engine,
    playback,
    motionNames,
    selectedName,
    selectMotion: (name) => {
      setPendingName(null);
      setSelectedName(name);
    },
    createMotion: (name, model, clip) => {
      setPendingName(name);
      createMotion(name, model, clip);
    },
    settings,
    corrections,
    isCyclic: settings?.loop !== false,
    view,
    setView,
    info,
    error,
    analysis,
    selectedBone,
    selectBone: setSelectedBone,
    preview,
    previewNow,
    commit,
    setGestureActive: (isActive) => setReloadGateOpen(!isActive),
  };
}
