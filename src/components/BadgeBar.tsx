import { useMemo } from "react";
import { BadgeGroup } from "./BadgeGroup";
import { sortAndAssignBadges } from "../lib/badgeSort";
import type { AgentSession, NotchInfo } from "../types";

type ViewState = "hidden" | "collapsed" | "expanded";

interface BadgeBarProps {
  sessions: AgentSession[];
  notchInfo: NotchInfo;
  viewState: ViewState;
  totalCost: number;
}

export function BadgeBar({ sessions, notchInfo, viewState }: BadgeBarProps) {
  const { left, right, overflowCount } = useMemo(
    () => sortAndAssignBadges(sessions),
    [sessions],
  );

  const notchHalfWidth = notchInfo.width / 2;
  const sideGap = 4;

  if (left.length === 0 && right.length === 0) return null;

  return (
    <div
      className={`badge-bar badge-bar--${viewState}`}
    >
      <div
        className="badge-bar__left"
        style={{ right: `calc(50% + ${notchHalfWidth + sideGap}px)` }}
      >
        <BadgeGroup sessions={left} side="left" />
      </div>
      <div
        className="badge-bar__right"
        style={{ left: `calc(50% + ${notchHalfWidth + sideGap}px)` }}
      >
        <BadgeGroup sessions={right} side="right" />
        {overflowCount > 0 && (
          <span className="badge-overflow">…</span>
        )}
      </div>
    </div>
  );
}
