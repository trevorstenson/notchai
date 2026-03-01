import { STATUS_COLORS } from "../types";
import type { AgentStatus } from "../types";

interface StatusDotProps {
  status: AgentStatus;
  size?: number;
}

export function StatusDot({ status, size = 8 }: StatusDotProps) {
  const color = STATUS_COLORS[status];
  const isOperating = status === "operating";
  const isWaitingForApproval = status === "waitingForApproval";
  const animClass = isOperating
    ? "status-dot--pulse"
    : isWaitingForApproval
      ? "status-dot--blink"
      : "";

  return (
    <span
      className={`status-dot ${animClass}`}
      style={{
        width: size,
        height: size,
        backgroundColor: color,
        boxShadow: `0 0 ${isOperating || isWaitingForApproval ? 6 : 3}px ${color}80`,
      }}
    />
  );
}
