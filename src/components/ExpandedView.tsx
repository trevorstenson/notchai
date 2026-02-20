import { AgentRow } from "./AgentRow";
import type { AgentSession } from "../types";

interface ExpandedViewProps {
  sessions: AgentSession[];
  selectedId: string | null;
  onSelect: (id: string | null) => void;
}

export function ExpandedView({
  sessions,
  selectedId,
  onSelect,
}: ExpandedViewProps) {
  if (sessions.length === 0) {
    return (
      <div className="expanded-view">
        <div className="expanded-empty">
          <span className="expanded-empty__icon">◇</span>
          <span>No active agents</span>
        </div>
      </div>
    );
  }

  return (
    <div className="expanded-view">
      <div className="expanded-header">
        <span className="expanded-title">Agents</span>
        <span className="expanded-count">{sessions.length}</span>
      </div>
      <div className="expanded-list">
        {sessions.map((session) => (
          <AgentRow
            key={session.id}
            session={session}
            isSelected={session.id === selectedId}
            onClick={() =>
              onSelect(session.id === selectedId ? null : session.id)
            }
          />
        ))}
      </div>
    </div>
  );
}
