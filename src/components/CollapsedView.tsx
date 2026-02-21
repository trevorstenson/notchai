import { StatusDot } from "./StatusDot";
import type { AgentSession } from "../types";

interface CollapsedViewProps {
  sessions: AgentSession[];
  operatingCount: number;
  debugLabel?: string;
}

export function CollapsedView({ sessions, operatingCount, debugLabel }: CollapsedViewProps) {
  const activeSessions = sessions.filter((s) => s.status !== "completed");
  const visibleSessions = activeSessions.length > 0 ? activeSessions : sessions;

  return (
    <div className="collapsed-content">
      <div className="collapsed-dots">
        {visibleSessions.slice(0, 5).map((session) => (
          <StatusDot key={session.id} status={session.status} size={7} />
        ))}
      </div>
      {operatingCount > 0 ? (
        <span className="collapsed-count">{operatingCount} active</span>
      ) : activeSessions.length > 0 ? (
        <span className="collapsed-count">{activeSessions.length} idle</span>
      ) : sessions.length > 0 ? (
        <span className="collapsed-count">{sessions.length} recent</span>
      ) : (
        <span className="collapsed-empty">notchai</span>
      )}
      {debugLabel && <span className="collapsed-debug">{debugLabel}</span>}
    </div>
  );
}
