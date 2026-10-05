import { useRef, useState } from "react";
import { EditorIcon } from "./EditorIcon";
import { EditorLogoMark } from "./EditorLogoMark";
import type { SaveState } from "./editSession";
import type { ProjectSource } from "./projectSource";
import { ToolMenu, ToolMenuItem } from "./ToolMenu";
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
  /** Место над сценой, куда встают инструменты трёхмерной сцены. */
  onToolbarSlotChange: (slot: HTMLElement | null) => void;
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
  const label = `Обновлено в ${loadedAt.toLocaleTimeString("ru-RU")}. Редактор сам перечитывает проект, когда его файлы меняются`;
  // `key` по времени перезапускает вспышку точки на каждой перезагрузке — видно, что правка подхвачена.
  return (
    <span key={loadedAt.getTime()} className="project-topbar__live project-topbar__live--flash" role="img" title={label} aria-label={label}>
      <span className="project-topbar__live-dot" />
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

/**
 * «Запуск/Стоп», «Пауза», «Шаг» значками, шкала повтора, меню записей и звук — «Редактор», требования 1, 27–29, 34–35.
 * «Повтор», «Сохранить запись» и «Открыть запись» нужны реже — они в меню «⋮», а не кнопками в полосе.
 */
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
        className="editor-button editor-button--icon battle-transport__play"
        title={isInSession ? "Стоп (Ctrl+P)" : "Запуск (Ctrl+P)"}
        aria-label={isInSession ? "Стоп" : "Запуск"}
        disabled={!isInSession && !battle.canPlay}
        onClick={isInSession ? battle.stop : battle.play}
      >
        <EditorIcon name={isInSession ? "stop" : "play"} size={15} />
      </button>
      <button
        type="button"
        className="editor-button editor-button--icon"
        title={battle.isRunning || !isInSession ? "Пауза (Ctrl+Shift+P)" : "Продолжить (Ctrl+Shift+P)"}
        aria-label={battle.isRunning || !isInSession ? "Пауза" : "Продолжить"}
        disabled={!isInSession || (!battle.isRunning && battle.codeError !== null)}
        onClick={battle.pauseOrResume}
      >
        <EditorIcon name={battle.isRunning || !isInSession ? "pause" : "play"} size={15} />
      </button>
      <button
        type="button"
        className="editor-button editor-button--icon"
        title={battle.stepBlockedReason ?? "Шаг (Ctrl+Alt+P)"}
        aria-label="Шаг"
        disabled={battle.stepBlockedReason !== undefined}
        onClick={battle.step}
      >
        <EditorIcon name="step-forward" size={15} />
      </button>

      {battle.mode === "replay" && (
        <>
          <button type="button" className="editor-button editor-button--icon" title="Шаг назад (←)" aria-label="Шаг назад" disabled={battle.currentStep <= 0} onClick={battle.stepBack}>
            <EditorIcon name="step-back" size={14} />
          </button>
          <ReplayScrubber battle={battle} draftStep={effectiveDraftStep} onDraftChange={setDraftStep} onCommit={commitScrubber} />
        </>
      )}

      {isInSession && <span className="battle-transport__step-label">{battleStatusLabel(battle, effectiveDraftStep ?? battle.currentStep)}</span>}

      <ToolMenu label="Записи партий" icon="more" title="Повтор и записи партий">
        {(close) => (
          <div className="tool-menu__items">
            {battle.mode === "edit" && (
              <ToolMenuItem
                icon="replay"
                label="Повтор"
                hint="последней партии"
                isDisabled={!battle.canStartReplay}
                onSelect={() => {
                  close();
                  battle.startReplay();
                }}
              />
            )}
            <ToolMenuItem
              icon="download"
              label="Сохранить запись"
              isDisabled={!battle.hasRecording}
              onSelect={() => {
                close();
                void battle.saveRecording();
              }}
            />
            <ToolMenuItem
              icon="folder"
              label="Открыть запись"
              onSelect={() => {
                close();
                fileInputRef.current?.click();
              }}
            />
          </div>
        )}
      </ToolMenu>
      <input ref={fileInputRef} type="file" accept=".json" hidden onChange={handleOpenReplayFile} />

      <button
        type="button"
        className="editor-button editor-button--icon"
        title={battle.isMuted ? "Включить звук" : "Без звука"}
        aria-label={battle.isMuted ? "Включить звук" : "Без звука"}
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
 * Верхняя полоса окна проекта — «Редактор», требование 8: одна строка из трёх частей над тремя колонками окна. Над
 * списком объектов — знак, «К проектам», название проекта и «Отменить»; над сценой — инструменты трёхмерной сцены
 * (их ставит сцена в `onToolbarSlotChange`) и кнопки партии; над свойствами — строки о записи и точка «Обновлено».
 * «Отменить» неактивна без истории (требования 2, 4, 21); во время партии и повтора полоса подсвечена (требование 5).
 */
export function ProjectTopBar({ projectName, source, headerNotice, loadedAt, saveState, canUndo, onUndo, onBackToProjects, battle, onToolbarSlotChange }: ProjectTopBarProps): React.JSX.Element {
  const sourceTitle = source.kind === "listed" ? `Проект из списка: games/${source.name}` : `Папка с диска: ${source.displayName}`;
  return (
    <header className={`project-topbar${battle.mode !== "edit" ? ` project-topbar--${battle.mode}` : ""}`}>
      <div className="project-topbar__project">
        <EditorLogoMark size={24} />
        <button type="button" className="editor-button editor-button--icon" title="К проектам" aria-label="К проектам" onClick={onBackToProjects}>
          <EditorIcon name="arrow-left" size={15} />
        </button>
        <h1 className="project-topbar__title" title={sourceTitle}>
          {projectName}
        </h1>
        <button type="button" className="editor-button editor-button--icon" disabled={!canUndo} title="Отменить (Ctrl+Z)" aria-label="Отменить" onClick={onUndo}>
          <EditorIcon name="undo" size={15} />
        </button>
      </div>
      <div className="project-topbar__scene">
        <div ref={onToolbarSlotChange} className="scene-tools" />
        <BattleTransportControls battle={battle} />
      </div>
      <div className="project-topbar__status">
        <BattleNotices battle={battle} />
        <SaveStateNotice saveState={saveState} />
        <ProjectStatus headerNotice={headerNotice} loadedAt={loadedAt} />
      </div>
    </header>
  );
}
