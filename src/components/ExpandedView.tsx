import { StatusDot } from "./StatusDot";
import { STATUS_LABELS } from "../types";
import type { AgentSession } from "../types";

interface ExpandedViewProps {
  sessions: AgentSession[];
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

export function ExpandedView({ sessions }: ExpandedViewProps) {
  if (sessions.length === 0) {
    return (
      <div className="expanded-content">
        <div className="expanded-empty">No active sessions</div>
      </div>
    );
  }

  return (
    <div className="expanded-content">
      <div className="expanded-divider" />
      <div className="expanded-sessions">
        {sessions.slice(0, 8).map((session) => (
          <div key={session.id} className="session-row">
            <StatusDot status={session.status} size={8} />
            <span className="session-name">{session.projectName}</span>
            <span className="session-status">
              {STATUS_LABELS[session.status]}
            </span>
            <span className="session-time">{timeAgo(session.modified)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
