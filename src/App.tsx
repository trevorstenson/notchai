import { useState, useCallback, useEffect, useRef, useMemo } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useAgentMonitor } from "./hooks/useAgentMonitor";
import { useSessionNotifications } from "./hooks/useSessionNotifications";
import { ExpandedView } from "./components/ExpandedView";
import { SettingsView } from "./components/SettingsView";
import { ToolApproval } from "./components/ToolApproval";
import { BadgeBar } from "./components/BadgeBar";
import { calculateSessionCost } from "./lib/pricing";
import "./App.css";

type ViewState = "hidden" | "collapsed" | "expanded" | "settings";

const LEAVE_COLLAPSE_DELAY_MS = 120;

// Debug behavior is enabled during development only.
const DEBUG_MODE = import.meta.env.DEV;
const INITIAL_VIEW_STATE: ViewState = "collapsed";

function App() {
  const [viewState, setViewState] = useState<ViewState>(INITIAL_VIEW_STATE);
  const animatingRef = useRef(false);
  const animatingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const leaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const { sessions, notchInfo, pendingApprovals, respondToApproval } =
    useAgentMonitor(3000, animatingRef);
  useSessionNotifications(sessions);

  const hasPendingApprovals = pendingApprovals.length > 0;

  const [autoExpandOnApproval, setAutoExpandOnApproval] = useState(true);

  // Load auto-expand setting on mount
  useEffect(() => {
    invoke<{
      autoExpandOnApproval: boolean;
    }>("get_settings")
      .then((settings) => {
        setAutoExpandOnApproval(settings.autoExpandOnApproval);
      })
      .catch(console.error);
  }, []);

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
    if (viewState === "expanded" || viewState === "settings") return;
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

  const openSettings = useCallback(() => {
    debugLog("[notchai-ui] openSettings");
    clearLeaveTimer();
    if (animatingTimerRef.current) {
      clearTimeout(animatingTimerRef.current);
    }
    animatingRef.current = true;
    setViewState("settings");
    animatingTimerRef.current = setTimeout(() => {
      animatingRef.current = false;
      animatingTimerRef.current = null;
    }, 400);
  }, [debugLog, clearLeaveTimer]);

  const closeSettings = useCallback(() => {
    debugLog("[notchai-ui] closeSettings");
    setViewState("expanded");
  }, [debugLog]);

  const handleIslandMouseEnter = useCallback(() => {
    clearLeaveTimer();
    debugLog("[notchai-ui] mouseenter", { viewState });
    expandPanel();
  }, [clearLeaveTimer, debugLog, expandPanel, viewState]);

  const handleIslandMouseLeave = useCallback(() => {
    clearLeaveTimer();
    debugLog("[notchai-ui] mouseleave", { viewState });
    if (viewState === "settings") return;
    leaveTimerRef.current = setTimeout(() => {
      collapsePanel();
      leaveTimerRef.current = null;
    }, LEAVE_COLLAPSE_DELAY_MS);
  }, [clearLeaveTimer, collapsePanel, debugLog, viewState]);

  // Auto-expand when pending approvals arrive (if setting enabled)
  useEffect(() => {
    if (hasPendingApprovals && autoExpandOnApproval) {
      clearLeaveTimer();
      expandPanel();
    }
  }, [hasPendingApprovals, autoExpandOnApproval, clearLeaveTimer, expandPanel]);

  useEffect(() => {
    const passthrough = viewState === "collapsed" || viewState === "hidden";
    invoke("set_window_mouse_passthrough", { enabled: passthrough }).catch((err) => {
      console.error("[notchai-ui] set_window_mouse_passthrough failed", err);
    });
  }, [viewState]);

  useEffect(() => {
    const unlistenOpen = listen("open-panel", () => {
      clearLeaveTimer();
      expandPanel();
    });
    const unlistenClose = listen("close-panel", () => {
      if (viewState === "settings") return;
      clearLeaveTimer();
      collapsePanel();
    });
    return () => {
      clearLeaveTimer();
      unlistenOpen.then((fn) => fn());
      unlistenClose.then((fn) => fn());
    };
  }, [clearLeaveTimer, collapsePanel, expandPanel, viewState]);

  const handleSessionOpened = useCallback(() => {
    clearLeaveTimer();
    collapsePanel();
  }, [clearLeaveTimer, collapsePanel]);

  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
  } | null>(null);

  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setContextMenu({ x: e.clientX, y: e.clientY });
    },
    []
  );

  const handleContextMenuClose = useCallback(() => {
    setContextMenu(null);
  }, []);

  const handleContextMenuSettings = useCallback(() => {
    setContextMenu(null);
    openSettings();
  }, [openSettings]);

  // Determine island CSS class — settings uses expanded dimensions
  const islandSizeClass =
    viewState === "settings" ? "island--expanded" : `island--${viewState}`;

  return (
    <div
      className={`notch-root notch-root--${viewState === "settings" ? "expanded" : viewState}`}
      onClick={contextMenu ? handleContextMenuClose : undefined}
    >
      {notchInfo && (
        <BadgeBar
          sessions={sessions}
          notchInfo={notchInfo}
          viewState={viewState === "settings" ? "expanded" : viewState}
          totalCost={totalCost}
        />
      )}
      <div className="island-wrapper">
        <div
          className={`island ${islandSizeClass}`}
          onMouseEnter={handleIslandMouseEnter}
          onMouseLeave={handleIslandMouseLeave}
          onContextMenu={handleContextMenu}
        >
          {viewState !== "hidden" && (
            <>
              {hasPendingApprovals && viewState !== "settings" && (
                <ToolApproval
                  pendingApprovals={pendingApprovals}
                  respondToApproval={respondToApproval}
                />
              )}
              {viewState === "settings" ? (
                <SettingsView
                  onBack={closeSettings}
                  autoExpandOnApproval={autoExpandOnApproval}
                  onToggleAutoExpand={() => setAutoExpandOnApproval((v) => !v)}
                />
              ) : (
                <ExpandedView
                  sessions={sessions}
                  onSessionOpened={handleSessionOpened}
                  onOpenSettings={openSettings}
                />
              )}
            </>
          )}
        </div>
      </div>

      {contextMenu && (
        <div
          className="context-menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
        >
          <button
            className="context-menu-item"
            onClick={handleContextMenuSettings}
          >
            Settings
          </button>
        </div>
      )}
    </div>
  );
}

export default App;
