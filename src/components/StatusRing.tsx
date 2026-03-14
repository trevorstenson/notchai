import type { AgentStatus } from "../types";
import { STATUS_COLORS } from "../types";

interface StatusRingProps {
  status: AgentStatus;
  size?: number;
}

export function StatusRing({ status, size = 8 }: StatusRingProps) {
  const color = STATUS_COLORS[status];
  const r = (size - 1.5) / 2;
  const cx = size / 2;
  const cy = size / 2;

  const getClassName = () => {
    switch (status) {
      case "operating":
        return "status-ring--pulse";
      case "waitingForInput":
      case "waitingForApproval":
        return "status-ring--nudge";
      case "error":
        return "status-ring--error-flash";
      default:
        return "";
    }
  };

  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      className={`status-ring ${getClassName()}`}
    >
      {status === "idle" && (
        <circle
          cx={cx}
          cy={cy}
          r={r}
          fill="none"
          stroke={color}
          strokeWidth={1.5}
          opacity={0.5}
        />
      )}
      {status === "operating" && (
        <circle cx={cx} cy={cy} r={r} fill={color} />
      )}
      {status === "waitingForInput" && (
        <>
          <defs>
            <clipPath id={`half-clip-${size}`}>
              <rect x={0} y={0} width={size / 2} height={size} />
            </clipPath>
          </defs>
          <circle
            cx={cx}
            cy={cy}
            r={r}
            fill="none"
            stroke={color}
            strokeWidth={1.5}
            opacity={0.4}
          />
          <circle
            cx={cx}
            cy={cy}
            r={r}
            fill={color}
            clipPath={`url(#half-clip-${size})`}
          />
        </>
      )}
      {status === "waitingForApproval" && (
        <>
          <circle cx={cx} cy={cy} r={r} fill={color} />
          <text
            x={cx}
            y={cy + 0.5}
            textAnchor="middle"
            dominantBaseline="central"
            fill="#000"
            fontSize={size * 0.65}
            fontWeight="bold"
            fontFamily="var(--font-sans)"
          >
            !
          </text>
        </>
      )}
      {status === "error" && (
        <circle
          cx={cx}
          cy={cy}
          r={r}
          fill="none"
          stroke={color}
          strokeWidth={1.5}
          strokeDasharray={`${Math.PI * r * 0.7} ${Math.PI * r * 0.3}`}
        />
      )}
      {status === "completed" && (
        <circle cx={cx} cy={cy} r={r} fill="#666" opacity={0.6} />
      )}
    </svg>
  );
}
