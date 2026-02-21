import { StatusDot } from "./StatusDot";
import type { AgentSession } from "../types";

interface CollapsedViewProps {
  sessions: AgentSession[];
  operatingCount: number;
}

export function CollapsedView({ sessions, operatingCount }: CollapsedViewProps) {
  const activeSessions = sessions.filter((s) => s.status !== "completed");
  const waitingCount = sessions.filter((s) => s.status === "waitingForInput").length;
  const visibleSessions = activeSessions.length > 0 ? activeSessions : sessions;

  return (
    <div className="collapsed-content">
      <div className="collapsed-dots">
        {visibleSessions.slice(0, 5).map((session) => (
          <StatusDot key={session.id} status={session.status} size={7} />
        ))}
      </div>
      {waitingCount > 0 ? (
        <span className="collapsed-count collapsed-count--waiting">
          {waitingCount} needs action
        </span>
      ) : operatingCount > 0 ? (
        <span className="collapsed-count">{operatingCount} active</span>
      ) : activeSessions.length > 0 ? (
        <span className="collapsed-count">{activeSessions.length} idle</span>
      ) : sessions.length > 0 ? (
        <span className="collapsed-count">{sessions.length} recent</span>
      ) : (
        <span className="collapsed-empty">notchai</span>
      )}
    </div>
  );
}
