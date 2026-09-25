import type { SessionMessage } from "./battleTypes";

type MessagesPanelProps = { messages: SessionMessage[] };

/** Вкладка «Сообщения» — «Редактор», требование 26: `print` кода игры и сбои движка по шагам. */
export function MessagesPanel({ messages }: MessagesPanelProps): React.JSX.Element {
  if (messages.length === 0) {
    return <div className="problems-panel problems-panel--empty">Сообщений нет</div>;
  }
  return (
    <ul className="messages-panel__list">
      {messages.map((message, index) => (
        <li key={index} className="messages-panel__line">
          <span className="messages-panel__step">шаг {message.step}</span>
          {message.text}
        </li>
      ))}
    </ul>
  );
}
