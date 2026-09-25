import { useRef, useState } from "react";
import { EditorIcon } from "./EditorIcon";
import { EditorLogoMark } from "./EditorLogoMark";
import type { SaveState } from "./editSession";
import type { ProjectSource } from "./projectSource";
import type { BattleSessionState } from "./useBattleSession";

type ProjectTopBarProps = {
  projectName: string;
  source: ProjectSource;
  headerNotice: string | null;
  /** Когда проект последний раз загрузился или перезагрузился после правки файлов. */
  loadedAt: Date | null;
  saveState: SaveState;
  canUndo: boolean;
  onUndo: () => void;
  onBackToProjects: () => void;
  battle: BattleSessionState;
};

function ProjectStatus({ headerNotice, loadedAt }: Pick<ProjectTopBarProps, "headerNotice" | "loadedAt">): React.JSX.Element | null {
  if (headerNotice !== null) {
    return (
      <span className="project-topbar__notice">
        <EditorIcon name="warning" size={14} />
        {headerNotice}
      </span>
    );
  }
  if (loadedAt === null) return null;
  // `key` по времени перезапускает вспышку строки на каждой перезагрузке — видно, что правка подхвачена.
  return (
    <span key={loadedAt.getTime()} className="project-topbar__live project-topbar__live--flash" title="Редактор сам перечитывает проект, когда его файлы меняются">
      <span className="project-topbar__live-dot" />
      Обновлено в {loadedAt.toLocaleTimeString("ru-RU")}
    </span>
  );
}

/** «Не сохранено — …» — «Редактор», требования 2, 4: строка появляется, пока показанный текст не записан. */
function SaveStateNotice({ saveState }: { saveState: SaveState }): React.JSX.Element | null {
  if (saveState.status === "saved") return null;
  return (
    <span className="project-topbar__notice" title={saveState.reason}>
      <EditorIcon name="warning" size={14} />
      Не сохранено — {saveState.reason}
    </span>
  );
}

/** «Партия · шаг N» / «Повтор · шаг N из M» — «Редактор», требование 6; `displayedStep` — номер, что
 *  показывает шкала прямо сейчас (черновик во время перетаскивания, иначе текущий шаг партии). */
function battleStatusLabel(battle: BattleSessionState, displayedStep: number): string {
  if (battle.mode === "battle") return `Партия · шаг ${battle.currentStep}`;
  return `Повтор · шаг ${displayedStep} из ${battle.recordingLength}`;
}

/**
 * Шкала повтора — «Редактор», требование 29: перетаскивание только двигает ползунок и номер шага
 * своим черновиком (`draftStep`, поднят в `BattleTransportControls`, чтобы им пользовалась и подпись
 * рядом), не пересчитывая партию на каждое дрожание мыши — каждый `seek` пересчитывает запись с
 * начала и стоит до секунды на длинной записи (нефункциональное требование). `seek` зовётся один
 * раз, при отпускании (`onPointerUp`) или по клавиатуре (`onKeyUp`); нативный `change` для
 * `<input type="range">` в React недоступен отдельно от `input` — эти два события и заменяют его здесь.
 */
function ReplayScrubber({
  battle,
  draftStep,
  onDraftChange,
  onCommit,
}: {
  battle: BattleSessionState;
  draftStep: number | null;
  onDraftChange: (step: number) => void;
  onCommit: (step: number) => void;
}): React.JSX.Element {
  function commit(event: React.SyntheticEvent<HTMLInputElement>): void {
    if (draftStep === null) return;
    onCommit(Number(event.currentTarget.value));
  }

  return (
    <input
      type="range"
      className="battle-transport__scrubber"
      min={0}
      max={battle.recordingLength}
      value={draftStep ?? battle.currentStep}
      aria-label="Шаг повтора"
      onChange={(event) => onDraftChange(Number(event.target.value))}
      onPointerUp={commit}
      onKeyUp={commit}
    />
  );
}

/** Кнопки «Запуск/Стоп», «Пауза», «Шаг», шкала повтора и звук — «Редактор», требования 1, 27–29, 34–35. */
function BattleTransportControls({ battle }: { battle: BattleSessionState }): React.JSX.Element {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [draftStep, setDraftStep] = useState<number | null>(null);

  function handleOpenReplayFile(event: React.ChangeEvent<HTMLInputElement>): void {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;
    void file.text().then((text) => battle.openReplayFile(text));
  }

  function commitScrubber(step: number): void {
    setDraftStep(null);
    battle.seek(step);
  }

  const isInSession = battle.mode !== "edit";
  // Черновик шкалы имеет смысл только в повторе — вышли из него (например, «Стоп» посреди
  // перетаскивания) не донеся до отпускания — не тащить устаревшее значение в следующий повтор.
  const effectiveDraftStep = battle.mode === "replay" ? draftStep : null;

  return (
    <div className="battle-transport">
      <button
        type="button"
        className="editor-button editor-button--outline"
        title={isInSession ? "Стоп (Ctrl+P)" : "Запуск (Ctrl+P)"}
        disabled={!isInSession && !battle.canPlay}
        onClick={isInSession ? battle.stop : battle.play}
      >
        <EditorIcon name={isInSession ? "stop" : "play"} size={14} />
        {isInSession ? "Стоп" : "Запуск"}
      </button>
      <button
        type="button"
        className="editor-button editor-button--outline"
        title={battle.isRunning ? "Пауза (Ctrl+Shift+P)" : "Продолжить (Ctrl+Shift+P)"}
        disabled={!isInSession || (!battle.isRunning && battle.codeError !== null)}
        onClick={battle.pauseOrResume}
      >
        <EditorIcon name={battle.isRunning ? "pause" : "play"} size={14} />
        {battle.isRunning ? "Пауза" : "Продолжить"}
      </button>
      <button
        type="button"
        className="editor-button editor-button--outline"
        title={battle.stepBlockedReason ?? "Шаг (Ctrl+Alt+P)"}
        disabled={battle.stepBlockedReason !== undefined}
        onClick={battle.step}
      >
        <EditorIcon name="step-forward" size={14} />
        Шаг
      </button>

      {battle.mode === "replay" && (
        <>
          <button type="button" className="editor-button" title="Шаг назад (←)" disabled={battle.currentStep <= 0} onClick={battle.stepBack}>
            <EditorIcon name="step-back" size={14} />
          </button>
          <ReplayScrubber battle={battle} draftStep={effectiveDraftStep} onDraftChange={setDraftStep} onCommit={commitScrubber} />
        </>
      )}

      {isInSession && <span className="battle-transport__step-label">{battleStatusLabel(battle, effectiveDraftStep ?? battle.currentStep)}</span>}

      {battle.mode === "edit" && (
        <button type="button" className="editor-button" title="Повтор" disabled={!battle.canStartReplay} onClick={battle.startReplay}>
          <EditorIcon name="replay" size={14} />
          Повтор
        </button>
      )}

      <button type="button" className="editor-button" title="Сохранить запись" disabled={!battle.hasRecording} onClick={() => void battle.saveRecording()}>
        <EditorIcon name="download" size={14} />
      </button>
      <button type="button" className="editor-button" title="Открыть запись" onClick={() => fileInputRef.current?.click()}>
        <EditorIcon name="folder" size={14} />
      </button>
      <input ref={fileInputRef} type="file" accept=".json" hidden onChange={handleOpenReplayFile} />

      <button
        type="button"
        className="editor-button"
        title={battle.isMuted ? "Включить звук" : "Без звука"}
        aria-pressed={battle.isMuted}
        onClick={battle.toggleMute}
      >
        <EditorIcon name={battle.isMuted ? "volume-off" : "volume-on"} size={14} />
      </button>
    </div>
  );
}

/** Строки о партии — файлы изменились, запись сохранена, запись не открылась, ошибка кода игры. */
function BattleNotices({ battle }: { battle: BattleSessionState }): React.JSX.Element | null {
  if (battle.codeError !== null) {
    return (
      <span className="project-topbar__notice project-topbar__notice--error" title={battle.codeError.message}>
        <EditorIcon name="error" size={14} />
        Ошибка кода игры на шаге {battle.currentStep}
      </span>
    );
  }
  if (battle.fileChangeNotice) {
    return (
      <span className="project-topbar__notice">
        <EditorIcon name="warning" size={14} />
        Файлы проекта изменились — применятся после остановки
      </span>
    );
  }
  if (battle.openReplayError !== null) {
    return (
      <span className="project-topbar__notice project-topbar__notice--error" title={battle.openReplayError}>
        <EditorIcon name="error" size={14} />
        {battle.openReplayError}
      </span>
    );
  }
  if (battle.saveNotice !== null) {
    return (
      <span className="project-topbar__notice">
        <EditorIcon name={battle.saveNotice.startsWith("Не сохранено") ? "warning" : "ok"} size={14} />
        {battle.saveNotice}
      </span>
    );
  }
  return null;
}

/**
 * Верхняя полоса окна проекта — «Редактор», требование 8: кнопка «К проектам» и название проекта.
 * «Отменить» и строка «Не сохранено» — требования 2, 4, 21: кнопка неактивна без истории. Кнопки
 * партии, повтора и звука — требования 1, 27–29, 34–35; во время партии и повтора полоса подсвечена
 * (требование 5) через модификатор `project-topbar--battle`/`project-topbar--replay`.
 */
export function ProjectTopBar({ projectName, source, headerNotice, loadedAt, saveState, canUndo, onUndo, onBackToProjects, battle }: ProjectTopBarProps): React.JSX.Element {
  return (
    <header className={`project-topbar${battle.mode !== "edit" ? ` project-topbar--${battle.mode}` : ""}`}>
      <EditorLogoMark size={26} />
      <button type="button" className="editor-button" onClick={onBackToProjects}>
        <EditorIcon name="arrow-left" size={15} />К проектам
      </button>
      <span className="project-topbar__divider" />
      <h1 className="project-topbar__title">{projectName}</h1>
      <span className="editor-chip" title={source.kind === "listed" ? "Проект из списка games/" : "Папка с диска"}>
        {source.kind === "listed" ? `games/${source.name}` : (
          <>
            <EditorIcon name="folder" size={12} />
            {source.displayName}
          </>
        )}
      </span>
      <button type="button" className="editor-button" disabled={!canUndo} title="Отменить (Ctrl+Z)" onClick={onUndo}>
        <EditorIcon name="undo" size={15} />
        Отменить
      </button>
      <span className="project-topbar__divider" />
      <BattleTransportControls battle={battle} />
      <div className="project-topbar__status">
        <BattleNotices battle={battle} />
        <SaveStateNotice saveState={saveState} />
        <ProjectStatus headerNotice={headerNotice} loadedAt={loadedAt} />
      </div>
    </header>
  );
}
