import { useMemo } from "react";
import type { AgentSession, NotchInfo } from "../types";
import { STATUS_COLORS } from "../types";

type ViewState = "hidden" | "collapsed" | "expanded";

interface NotchArcProps {
  sessions: AgentSession[];
  notchInfo: NotchInfo;
  viewState: ViewState;
}

// Island dimensions (must match CSS vars)
const ISLAND_WIDTH = 240;
const ISLAND_HEIGHT = 36;
const CORNER_RADIUS = 20; // matches island border-radius: 0 0 20px 20px
const GAP = 1.5; // pathLength units between segments
const HIGHLIGHT_LEN = 12; // pathLength units for comet highlight
const BASE_DUR = 2; // seconds for full-arc comet travel

const AGENT_TYPE_ORDER: Record<string, number> = {
  claude: 0,
  codex: 1,
  cursor: 2,
  gemini: 3,
};

function sortSessions(sessions: AgentSession[]): AgentSession[] {
  return [...sessions].sort((a, b) => {
    const typeA = AGENT_TYPE_ORDER[a.agentType] ?? 99;
    const typeB = AGENT_TYPE_ORDER[b.agentType] ?? 99;
    if (typeA !== typeB) return typeA - typeB;
    return a.id.localeCompare(b.id);
  });
}

export function NotchArc({ sessions, notchInfo, viewState }: NotchArcProps) {
  const windowWidth = useMemo(
    () => Math.max(notchInfo.width + 340, 540),
    [notchInfo.width],
  );

  const pathD = useMemo(() => {
    // Trace around the island (our UI element), not the physical notch
    const L = (windowWidth - ISLAND_WIDTH) / 2;
    const R = L + ISLAND_WIDTH;
    const B = ISLAND_HEIGHT;
    const r = CORNER_RADIUS;

    return [
      `M ${L},0`,
      `L ${L},${B - r}`,
      `Q ${L},${B} ${L + r},${B}`,
      `L ${R - r},${B}`,
      `Q ${R},${B} ${R},${B - r}`,
      `L ${R},0`,
    ].join(" ");
  }, [windowWidth]);

  const sorted = useMemo(() => sortSessions(sessions), [sessions]);

  const segments = useMemo(() => {
    const N = sorted.length;
    if (N === 0) return [];

    const segLength = (100 - (N - 1) * GAP) / N;

    return sorted.map((session, i) => ({
      session,
      length: segLength,
      offset: i * (segLength + GAP),
    }));
  }, [sorted]);

  if (segments.length === 0) return null;

  return (
    <svg
      className={`notch-arc notch-arc--${viewState}`}
      width={windowWidth}
      height={ISLAND_HEIGHT + 4}
      viewBox={`0 -2 ${windowWidth} ${ISLAND_HEIGHT + 4}`}
    >
      {segments.map(({ session, length, offset }) => (
        <g key={session.id}>
          {/* Base segment */}
          <path
            d={pathD}
            pathLength={100}
            fill="none"
            stroke={STATUS_COLORS[session.status]}
            strokeWidth={2.5}
            strokeLinecap="round"
            strokeDasharray={`${length} 100`}
            strokeDashoffset={-offset}
            className={`arc-segment arc-segment--${session.status}`}
          />

          {/* Traveling highlight — operating only */}
          {session.status === "operating" && (
            <path
              d={pathD}
              pathLength={100}
              fill="none"
              stroke="rgba(255,255,255,0.85)"
              strokeWidth={2.5}
              strokeLinecap="round"
              strokeDasharray={`${HIGHLIGHT_LEN} 100`}
              strokeDashoffset={-offset}
            >
              <animate
                attributeName="stroke-dashoffset"
                from={String(-offset)}
                to={String(-offset - length - HIGHLIGHT_LEN)}
                dur={`${(length / 100) * BASE_DUR}s`}
                repeatCount="indefinite"
              />
            </path>
          )}
        </g>
      ))}
    </svg>
  );
}
