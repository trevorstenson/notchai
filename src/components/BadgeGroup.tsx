import { Badge } from "./Badge";
import type { AgentSession } from "../types";

interface BadgeGroupProps {
  sessions: AgentSession[];
  side: "left" | "right";
}

export function BadgeGroup({ sessions, side }: BadgeGroupProps) {
  if (sessions.length === 0) return null;

  return (
    <div
      className={`badge-group badge-group--${side}`}
    >
      {sessions.map((session) => (
        <Badge key={session.id} session={session} />
      ))}
    </div>
  );
}
