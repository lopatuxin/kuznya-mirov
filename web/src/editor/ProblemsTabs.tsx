import { useState } from "react";
import type { SessionMessage, StepReport } from "./battleTypes";
import { ErrorsPanel } from "./ErrorsPanel";
import { MessagesPanel } from "./MessagesPanel";
import { StepReportPanel } from "./StepReportPanel";

type ProblemsTab = "errors" | "step" | "messages";

type ProblemsTabsProps = {
  errorLines: string[];
  warningLines: string[];
  isLoading: boolean;
  stepReport: StepReport | undefined;
  messages: SessionMessage[];
  onSelectObject: (id: number) => void;
};

const TAB_LABELS: Record<ProblemsTab, string> = { errors: "Ошибки", step: "Шаг", messages: "Сообщения" };

/**
 * Нижняя панель редактора — «Редактор», требование 23: вкладки «Ошибки», «Шаг» и «Сообщения». Шаг и
 * сообщения относятся к партии и повтору — вне них показывают «Шагов ещё не было»/«Сообщений нет».
 */
export function ProblemsTabs({ errorLines, warningLines, isLoading, stepReport, messages, onSelectObject }: ProblemsTabsProps): React.JSX.Element {
  const [activeTab, setActiveTab] = useState<ProblemsTab>("errors");
  const problemCount = errorLines.length + warningLines.length;

  return (
    <div className="problems-tabs">
      <div className="problems-tabs__header" role="tablist">
        {(Object.keys(TAB_LABELS) as ProblemsTab[]).map((tab) => (
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
      </div>
    </div>
  );
}
