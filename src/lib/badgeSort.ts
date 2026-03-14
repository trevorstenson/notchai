import type { AgentSession } from "../types";
import { STATUS_PRIORITY } from "../types";

export interface BadgeAssignment {
  left: AgentSession[];
  right: AgentSession[];
  overflowCount: number;
}

const MAX_PER_SIDE = 3;

export function sortAndAssignBadges(sessions: AgentSession[]): BadgeAssignment {
  if (sessions.length === 0) {
    return { left: [], right: [], overflowCount: 0 };
  }

  // Filter out completed sessions — they don't need badges
  const active = sessions.filter((s) => s.status !== "completed");
  if (active.length === 0) {
    return { left: [], right: [], overflowCount: 0 };
  }

  // Sort by priority desc, then by session ID for stability
  const sorted = [...active].sort((a, b) => {
    const pa = STATUS_PRIORITY[a.status] ?? 0;
    const pb = STATUS_PRIORITY[b.status] ?? 0;
    if (pb !== pa) return pb - pa;
    return a.id.localeCompare(b.id);
  });

  const maxVisible = MAX_PER_SIDE * 2;
  const visible = sorted.slice(0, maxVisible);
  const overflowCount = Math.max(0, sorted.length - maxVisible);

  // Alternate: even indices → left, odd → right
  const left: AgentSession[] = [];
  const right: AgentSession[] = [];

  visible.forEach((session, i) => {
    if (i % 2 === 0) {
      left.push(session);
    } else {
      right.push(session);
    }
  });

  return { left, right, overflowCount };
}
