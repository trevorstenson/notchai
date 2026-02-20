import { StatusDot } from "./StatusDot";
import { STATUS_LABELS } from "../types";
import type { AgentSession } from "../types";

interface AgentRowProps {
  session: AgentSession;
  onClick: () => void;
  isSelected: boolean;
}

function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1)}M`;
  if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(1)}K`;
  return String(tokens);
}

function timeAgo(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);

  if (diffMins < 1) return "now";
  if (diffMins < 60) return `${diffMins}m`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h`;
  return `${Math.floor(diffHours / 24)}d`;
}

export function AgentRow({ session, onClick, isSelected }: AgentRowProps) {
  const description =
    session.currentTask || session.summary || session.firstPrompt;

  return (
    <button
      className={`agent-row ${isSelected ? "agent-row--selected" : ""}`}
      onClick={onClick}
    >
      <StatusDot status={session.status} size={10} />

      <div className="agent-row__info">
        <div className="agent-row__header">
          <span className="agent-row__name">{session.projectName}</span>
          <span className="agent-row__status">
            {STATUS_LABELS[session.status]}
          </span>
        </div>
        <div className="agent-row__description">{description}</div>
      </div>

      <div className="agent-row__meta">
        {session.totalOutputTokens > 0 && (
          <span className="agent-row__tokens">
            {formatTokens(session.totalOutputTokens)}
          </span>
        )}
        <span className="agent-row__time">{timeAgo(session.modified)}</span>
      </div>
    </button>
  );
}
