import { useState } from "react";
import type { SessionMessage, StepReport } from "./battleTypes";
import { ErrorsPanel } from "./ErrorsPanel";
import { ImagesPanel } from "./ImagesPanel";
import { MessagesPanel } from "./MessagesPanel";
import type { ProjectImageTile } from "./projectImages";
import { StepReportPanel } from "./StepReportPanel";

type ProblemsTab = "errors" | "step" | "messages" | "images" | "particles";

type ProblemsTabsProps = {
  errorLines: string[];
  warningLines: string[];
  isLoading: boolean;
  stepReport: StepReport | undefined;
  messages: SessionMessage[];
  imageTiles: readonly ProjectImageTile[];
  /** Вкладка «Частицы» — только в плоской сцене; `null` — вкладки нет. */
  particlesPanel: React.ReactNode | null;
  onSelectObject: (id: number) => void;
};

const TAB_LABELS: Record<ProblemsTab, string> = { errors: "Ошибки", step: "Шаг", messages: "Сообщения", images: "Картинки", particles: "Частицы" };

/**
 * Нижняя панель редактора — «Редактор», требование 23: вкладки «Ошибки», «Шаг», «Сообщения» и «Картинки», в плоской сцене
 * ещё «Частицы». Шаг и сообщения относятся к партии и повтору — вне них показывают «Шагов ещё не было»/«Сообщений нет».
 */
export function ProblemsTabs({ errorLines, warningLines, isLoading, stepReport, messages, imageTiles, particlesPanel, onSelectObject }: ProblemsTabsProps): React.JSX.Element {
  const [selectedTab, setActiveTab] = useState<ProblemsTab>("errors");
  const tabs = (Object.keys(TAB_LABELS) as ProblemsTab[]).filter((tab) => tab !== "particles" || particlesPanel !== null);
  // Вкладка «Частицы» пропала вместе с плоской сценой (проект сменился) — открыта первая.
  const activeTab = tabs.includes(selectedTab) ? selectedTab : "errors";
  const problemCount = errorLines.length + warningLines.length;

  return (
    <div className="problems-tabs">
      <div className="problems-tabs__header" role="tablist">
        {tabs.map((tab) => (
          <button
            key={tab}
            type="button"
            role="tab"
            aria-selected={activeTab === tab}
            className={activeTab === tab ? "problems-tabs__tab problems-tabs__tab--active" : "problems-tabs__tab"}
            onClick={() => setActiveTab(tab)}
          >
            {TAB_LABELS[tab]}
            {tab === "errors" && problemCount > 0 && <span className="editor-count">{problemCount}</span>}
            {tab === "messages" && messages.length > 0 && <span className="editor-count">{messages.length}</span>}
          </button>
        ))}
      </div>
      <div className="problems-tabs__body">
        {activeTab === "errors" && <ErrorsPanel errorLines={errorLines} warningLines={warningLines} isLoading={isLoading} />}
        {activeTab === "step" && <StepReportPanel report={stepReport} onSelectObject={onSelectObject} />}
        {activeTab === "messages" && <MessagesPanel messages={messages} />}
        {activeTab === "images" && <ImagesPanel tiles={imageTiles} />}
        {activeTab === "particles" && particlesPanel}
      </div>
    </div>
  );
}
