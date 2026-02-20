import { StatusDot } from "./StatusDot";
import type { AgentSession } from "../types";

interface CollapsedViewProps {
  sessions: AgentSession[];
  operatingCount: number;
}

export function CollapsedView({ sessions, operatingCount }: CollapsedViewProps) {
  const activeSessions = sessions.filter((s) => s.status !== "completed");

  return (
    <div className="collapsed-view">
      <div className="collapsed-pill">
        <div className="collapsed-dots">
          {activeSessions.slice(0, 5).map((session) => (
            <StatusDot key={session.id} status={session.status} size={6} />
          ))}
        </div>
        {operatingCount > 0 && (
          <span className="collapsed-count">{operatingCount}</span>
        )}
        {activeSessions.length === 0 && (
          <span className="collapsed-empty">notchai</span>
        )}
      </div>
    </div>
  );
}
