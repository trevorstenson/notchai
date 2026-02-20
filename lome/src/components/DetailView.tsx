import { StatusDot } from "./StatusDot";
import { STATUS_COLORS, STATUS_LABELS } from "../types";
import type { AgentSession } from "../types";

interface DetailViewProps {
  session: AgentSession;
  onClose: () => void;
}

function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1)}M`;
  if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(1)}K`;
  return String(tokens);
}

export function DetailView({ session, onClose }: DetailViewProps) {
  return (
    <div className="detail-view">
      <div className="detail-header">
        <div className="detail-header__left">
          <StatusDot status={session.status} size={12} />
          <span className="detail-header__name">{session.projectName}</span>
        </div>
        <button className="detail-close" onClick={onClose}>
          ×
        </button>
      </div>

      <div className="detail-status">
        <span
          className="detail-status__badge"
          style={{
            color: STATUS_COLORS[session.status],
            borderColor: `${STATUS_COLORS[session.status]}40`,
          }}
        >
          {STATUS_LABELS[session.status]}
        </span>
        {session.model && (
          <span className="detail-model">{session.model}</span>
        )}
      </div>

      <div className="detail-stats">
        <div className="detail-stat">
          <span className="detail-stat__label">Messages</span>
          <span className="detail-stat__value">{session.messageCount}</span>
        </div>
        <div className="detail-stat">
          <span className="detail-stat__label">Input</span>
          <span className="detail-stat__value">
            {formatTokens(session.totalInputTokens)}
          </span>
        </div>
        <div className="detail-stat">
          <span className="detail-stat__label">Output</span>
          <span className="detail-stat__value">
            {formatTokens(session.totalOutputTokens)}
          </span>
        </div>
      </div>

      {session.gitBranch && (
        <div className="detail-branch">
          <span className="detail-branch__icon">⎇</span>
          <span className="detail-branch__name">{session.gitBranch}</span>
        </div>
      )}

      <div className="detail-prompt">
        <span className="detail-prompt__label">Prompt</span>
        <p className="detail-prompt__text">{session.firstPrompt}</p>
      </div>

      {session.summary && (
        <div className="detail-summary">
          <span className="detail-summary__label">Summary</span>
          <p className="detail-summary__text">{session.summary}</p>
        </div>
      )}

      <div className="detail-path">
        <span className="detail-path__text">{session.projectPath}</span>
      </div>
    </div>
  );
}
