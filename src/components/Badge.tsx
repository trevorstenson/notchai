import { StatusRing } from "./StatusRing";
import type { AgentSession } from "../types";
import { AGENT_COLORS } from "../types";

interface BadgeProps {
  session: AgentSession;
}

export function Badge({ session }: BadgeProps) {
  const monogram = session.projectName
    ? session.projectName.charAt(0).toUpperCase()
    : "?";

  const isApprovalGlow = session.status === "waitingForApproval";
  const glowColor = AGENT_COLORS[session.agentType];

  return (
    <div
      className={`badge ${isApprovalGlow ? "badge--approval-glow" : ""}`}
      title={`${session.projectName} — ${session.agentType} — ${session.status}`}
      style={{
        borderBottom: `2px solid ${glowColor}`,
        ...(isApprovalGlow ? { "--glow-color": glowColor } as React.CSSProperties : {}),
      }}
    >
      <span className="badge__monogram">{monogram}</span>
      <StatusRing status={session.status} size={8} />
    </div>
  );
}
