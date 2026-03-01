import { useState, useCallback, useEffect, useRef, useMemo } from "react";
import { listen } from "@tauri-apps/api/event";
import { useAgentMonitor } from "./hooks/useAgentMonitor";
import { useSessionNotifications } from "./hooks/useSessionNotifications";
import { CollapsedView } from "./components/CollapsedView";
import { ExpandedView } from "./components/ExpandedView";
import { ToolApproval } from "./components/ToolApproval";
import { NotchArc } from "./components/NotchArc";
import { calculateSessionCost } from "./lib/pricing";
import "./App.css";

type ViewState = "hidden" | "collapsed" | "expanded";

const LEAVE_COLLAPSE_DELAY_MS = 220;

// Debug behavior is enabled during development only.
const DEBUG_MODE = import.meta.env.DEV;

function App() {
  const [viewState, setViewState] = useState<ViewState>("collapsed");
  const animatingRef = useRef(false);
  const animatingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const leaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const { sessions, activeSessions, operatingCount, notchInfo, pendingApprovals, respondToApproval } =
    useAgentMonitor(3000, animatingRef);
  useSessionNotifications(sessions);

  const hasPendingApprovals = pendingApprovals.length > 0;

  const totalCost = useMemo(() => {
    const todayStart = new Date();
    todayStart.setHours(0, 0, 0, 0);
    const todayMs = todayStart.getTime();

    return sessions.reduce((sum, s) => {
      const created = new Date(s.created).getTime();
      if (Number.isNaN(created) || created < todayMs) return sum;
      return sum + calculateSessionCost(s.totalInputTokens, s.totalOutputTokens, s.model);
    }, 0);
  }, [sessions]);

  const debugLog = useCallback((message: string, payload?: unknown) => {
    if (DEBUG_MODE) {
      console.log(message, payload);
    }
  }, []);

  const clearLeaveTimer = useCallback(() => {
    if (leaveTimerRef.current) {
      clearTimeout(leaveTimerRef.current);
      leaveTimerRef.current = null;
    }
  }, []);

  const expandPanel = useCallback(() => {
    if (viewState === "expanded") return;
    debugLog("[notchai-ui] expandPanel");
    if (animatingTimerRef.current) {
      clearTimeout(animatingTimerRef.current);
    }
    animatingRef.current = true;
    setViewState("expanded");
    // Clear animating flag after CSS transition completes (360ms + buffer)
    animatingTimerRef.current = setTimeout(() => {
      animatingRef.current = false;
      animatingTimerRef.current = null;
    }, 400);
  }, [viewState, debugLog]);

  const collapsePanel = useCallback(() => {
    if (viewState === "collapsed") return;
    debugLog("[notchai-ui] collapsePanel");
    if (animatingTimerRef.current) {
      clearTimeout(animatingTimerRef.current);
      animatingTimerRef.current = null;
    }
    animatingRef.current = false;
    setViewState("collapsed");
  }, [viewState, debugLog]);

  const handleIslandMouseEnter = useCallback(() => {
    clearLeaveTimer();
    debugLog("[notchai-ui] mouseenter", { viewState });
    expandPanel();
  }, [clearLeaveTimer, debugLog, expandPanel, viewState]);

  const handleIslandMouseLeave = useCallback(() => {
    clearLeaveTimer();
    debugLog("[notchai-ui] mouseleave", { viewState });
    // Don't auto-collapse while there are pending approvals
    if (hasPendingApprovals) return;
    leaveTimerRef.current = setTimeout(() => {
      collapsePanel();
      leaveTimerRef.current = null;
    }, LEAVE_COLLAPSE_DELAY_MS);
  }, [clearLeaveTimer, collapsePanel, debugLog, viewState, hasPendingApprovals]);

  // Auto-expand when pending approvals arrive, prevent collapse via close-panel
  useEffect(() => {
    if (hasPendingApprovals) {
      clearLeaveTimer();
      expandPanel();
    }
  }, [hasPendingApprovals, clearLeaveTimer, expandPanel]);

  useEffect(() => {
    const unlistenOpen = listen("open-panel", () => {
      clearLeaveTimer();
      expandPanel();
    });
    const unlistenClose = listen("close-panel", () => {
      // Don't collapse while there are pending approvals
      if (hasPendingApprovals) return;
      clearLeaveTimer();
      collapsePanel();
    });
    return () => {
      clearLeaveTimer();
      unlistenOpen.then((fn) => fn());
      unlistenClose.then((fn) => fn());
    };
  }, [clearLeaveTimer, collapsePanel, expandPanel, hasPendingApprovals]);

  const handleSessionOpened = useCallback(() => {
    clearLeaveTimer();
    collapsePanel();
  }, [clearLeaveTimer, collapsePanel]);

  return (
    <div className={`notch-root notch-root--${viewState}`}>
      {notchInfo && activeSessions.length > 0 && (
        <NotchArc
          sessions={activeSessions}
          notchInfo={notchInfo}
          viewState={viewState}
        />
      )}
      <div className="island-wrapper">
        <div
          className={`island island--${viewState}`}
          onMouseEnter={handleIslandMouseEnter}
          onMouseLeave={handleIslandMouseLeave}
        >
          {viewState !== "hidden" && (
            <>
              <CollapsedView
                sessions={sessions}
                operatingCount={operatingCount}
                totalCost={totalCost}
                pendingApprovalCount={pendingApprovals.length}
              />
              {hasPendingApprovals && (
                <ToolApproval
                  pendingApprovals={pendingApprovals}
                  respondToApproval={respondToApproval}
                />
              )}
              <ExpandedView
                sessions={sessions}
                onSessionOpened={handleSessionOpened}
              />
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
