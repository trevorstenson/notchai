import { useState, useCallback, useEffect, useRef, useMemo } from "react";
import {
  getCurrentWindow,
  LogicalSize,
  LogicalPosition,
} from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { useAgentMonitor } from "./hooks/useAgentMonitor";
import { useSessionNotifications } from "./hooks/useSessionNotifications";
import { CollapsedView } from "./components/CollapsedView";
import { ExpandedView } from "./components/ExpandedView";
import { calculateSessionCost } from "./lib/pricing";
import "./App.css";

type ViewState = "hidden" | "collapsed" | "expanded";

// Hover zone — invisible mouse-capture area (matches lib.rs setup)
const HOVER_ZONE_WIDTH = 540;
const HOVER_ZONE_HEIGHT = 60;

// Active window — large enough for the expanded island
const ACTIVE_WIDTH = 540;
const ACTIVE_HEIGHT = 320;
const ENTER_EXPAND_DELAY_MS = 90;
const LEAVE_COLLAPSE_DELAY_MS = 220;
const OPEN_PANEL_EXPAND_DELAY_MS = 40;

// Debug behavior is enabled during development only.
const DEBUG_MODE = import.meta.env.DEV;
const DEBUG_FIXED_WINDOW = DEBUG_MODE;

function App() {
  const { sessions, operatingCount, notchInfo } =
    useAgentMonitor(1000);
  useSessionNotifications(sessions);

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

  const [viewState, setViewState] = useState<ViewState>("collapsed");
  const enterTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const leaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const debugLog = useCallback((message: string, payload?: unknown) => {
    if (DEBUG_MODE) {
      console.log(message, payload);
    }
  }, []);

  const getCenterX = useCallback(() => {
    return notchInfo ? notchInfo.x + notchInfo.width / 2 : 756;
  }, [notchInfo]);

  const resizeToHoverZone = useCallback(async () => {
    const win = getCurrentWindow();
    const centerX = getCenterX();
    debugLog("[notchai-ui] resizeToHoverZone", { centerX });
    const height = DEBUG_FIXED_WINDOW ? ACTIVE_HEIGHT : HOVER_ZONE_HEIGHT;
    await win.setSize(new LogicalSize(HOVER_ZONE_WIDTH, height));
    await win.setPosition(
      new LogicalPosition(centerX - HOVER_ZONE_WIDTH / 2, 0)
    );
  }, [debugLog, getCenterX]);

  const resizeToActive = useCallback(async () => {
    const win = getCurrentWindow();
    const centerX = getCenterX();
    debugLog("[notchai-ui] resizeToActive", { centerX });
    await win.setSize(new LogicalSize(ACTIVE_WIDTH, ACTIVE_HEIGHT));
    await win.setPosition(
      new LogicalPosition(centerX - ACTIVE_WIDTH / 2, 0)
    );
  }, [debugLog, getCenterX]);

  const clearInteractionTimers = useCallback(() => {
    if (enterTimerRef.current) {
      clearTimeout(enterTimerRef.current);
      enterTimerRef.current = null;
    }
    if (leaveTimerRef.current) {
      clearTimeout(leaveTimerRef.current);
      leaveTimerRef.current = null;
    }
  }, []);

  const expandPanel = useCallback(() => {
    if (viewState === "expanded") return;
    setViewState("expanded");
    if (!DEBUG_FIXED_WINDOW) {
      resizeToActive().catch((err) =>
        console.error("[notchai-ui] expand failed", err)
      );
    }
  }, [resizeToActive, viewState]);

  const collapsePanel = useCallback(() => {
    if (viewState === "collapsed") return;
    setViewState("collapsed");
    if (!DEBUG_FIXED_WINDOW) {
      resizeToHoverZone().catch((err) =>
        console.error("[notchai-ui] collapse failed", err)
      );
    }
  }, [resizeToHoverZone, viewState]);

  const handleIslandMouseEnter = useCallback(() => {
    clearInteractionTimers();
    debugLog("[notchai-ui] mouseenter", { viewState });
    enterTimerRef.current = setTimeout(() => {
      expandPanel();
      enterTimerRef.current = null;
    }, ENTER_EXPAND_DELAY_MS);
  }, [clearInteractionTimers, debugLog, expandPanel, viewState]);

  const handleIslandMouseLeave = useCallback(() => {
    clearInteractionTimers();
    debugLog("[notchai-ui] mouseleave", { viewState });
    leaveTimerRef.current = setTimeout(() => {
      collapsePanel();
      leaveTimerRef.current = null;
    }, LEAVE_COLLAPSE_DELAY_MS);
  }, [clearInteractionTimers, collapsePanel, debugLog, viewState]);

  // Debug mode: always keep notch zone visible (no hover requirement).
  useEffect(() => {
    if (DEBUG_FIXED_WINDOW) {
      resizeToActive().catch((err) =>
        console.error("[notchai-ui] initial debug resize failed", err)
      );
    } else {
      resizeToHoverZone().catch((err) =>
        console.error("[notchai-ui] initial hover resize failed", err)
      );
    }
    setViewState("collapsed");
  }, [resizeToActive, resizeToHoverZone]);

  useEffect(() => {
    const unlistenOpen = listen("open-panel", () => {
      clearInteractionTimers();
      enterTimerRef.current = setTimeout(() => {
        expandPanel();
        enterTimerRef.current = null;
      }, OPEN_PANEL_EXPAND_DELAY_MS);
    });
    const unlistenClose = listen("close-panel", () => {
      clearInteractionTimers();
      collapsePanel();
    });
    return () => {
      clearInteractionTimers();
      unlistenOpen.then((fn) => fn());
      unlistenClose.then((fn) => fn());
    };
  }, [clearInteractionTimers, collapsePanel, expandPanel]);

  const handleSessionOpened = useCallback(() => {
    clearInteractionTimers();
    collapsePanel();
  }, [clearInteractionTimers, collapsePanel]);

  return (
    <div className={`notch-root notch-root--${viewState}`}>
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
              />
              {viewState === "expanded" && (
                <ExpandedView
                  sessions={sessions}
                  onSessionOpened={handleSessionOpened}
                />
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
