import { useState, useCallback, useEffect, useRef } from "react";
import {
  getCurrentWindow,
  LogicalSize,
  LogicalPosition,
} from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { useAgentMonitor } from "./hooks/useAgentMonitor";
import { CollapsedView } from "./components/CollapsedView";
import { ExpandedView } from "./components/ExpandedView";
import "./App.css";

type ViewState = "hidden" | "collapsed" | "expanded";

// Hover zone — invisible mouse-capture area (matches lib.rs setup)
const HOVER_ZONE_WIDTH = 540;
const HOVER_ZONE_HEIGHT = 60;

// Active window — large enough for the expanded island
const ACTIVE_WIDTH = 540;
const ACTIVE_HEIGHT = 320;
const LEAVE_COLLAPSE_DELAY_MS = 180;
const DEBUG_FIXED_WINDOW = true;

function App() {
  const { sessions, operatingCount, notchInfo, error } =
    useAgentMonitor(2000);
  const [viewState, setViewState] = useState<ViewState>("collapsed");
  const [hoverEnterCount, setHoverEnterCount] = useState(0);
  const [hoverLeaveCount, setHoverLeaveCount] = useState(0);
  const [lastHoverEvent, setLastHoverEvent] = useState("none");
  const leaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const getCenterX = useCallback(() => {
    return notchInfo ? notchInfo.x + notchInfo.width / 2 : 756;
  }, [notchInfo]);

  const resizeToHoverZone = useCallback(async () => {
    const win = getCurrentWindow();
    const centerX = getCenterX();
    console.log("[notchai-ui] resizeToHoverZone", { centerX });
    const height = DEBUG_FIXED_WINDOW ? ACTIVE_HEIGHT : HOVER_ZONE_HEIGHT;
    await win.setSize(new LogicalSize(HOVER_ZONE_WIDTH, height));
    await win.setPosition(
      new LogicalPosition(centerX - HOVER_ZONE_WIDTH / 2, 0)
    );
  }, [getCenterX]);

  const resizeToActive = useCallback(async () => {
    const win = getCurrentWindow();
    const centerX = getCenterX();
    console.log("[notchai-ui] resizeToActive", { centerX });
    await win.setSize(new LogicalSize(ACTIVE_WIDTH, ACTIVE_HEIGHT));
    await win.setPosition(
      new LogicalPosition(centerX - ACTIVE_WIDTH / 2, 0)
    );
  }, [getCenterX]);

  const handleIslandMouseEnter = useCallback(() => {
    if (leaveTimerRef.current) {
      clearTimeout(leaveTimerRef.current);
      leaveTimerRef.current = null;
    }
    setHoverEnterCount((c) => c + 1);
    setLastHoverEvent(`enter@${new Date().toLocaleTimeString()}`);
    console.log("[notchai-ui] mouseenter", { viewState });
    if (viewState === "expanded") return;
    // Flip UI state immediately so expansion is visible even if window resize is slow.
    setViewState("expanded");
    if (!DEBUG_FIXED_WINDOW) {
      resizeToActive().catch((err) =>
        console.error("[notchai-ui] expand failed", err)
      );
    }
  }, [resizeToActive, viewState]);

  const handleIslandMouseLeave = useCallback(() => {
    setHoverLeaveCount((c) => c + 1);
    setLastHoverEvent(`leave@${new Date().toLocaleTimeString()}`);
    console.log("[notchai-ui] mouseleave", { viewState });
    if (viewState === "collapsed") return;
    leaveTimerRef.current = setTimeout(() => {
      setViewState("collapsed");
      if (!DEBUG_FIXED_WINDOW) {
        resizeToHoverZone().catch((err) =>
          console.error("[notchai-ui] collapse failed", err)
        );
      }
    }, LEAVE_COLLAPSE_DELAY_MS);
  }, [resizeToHoverZone, viewState]);

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
    const unlisten = listen("open-panel", () => {
      setViewState("expanded");
      if (!DEBUG_FIXED_WINDOW) {
        resizeToActive().catch((err) =>
          console.error("[notchai-ui] open-panel resize failed", err)
        );
      }
    });
    return () => {
      if (leaveTimerRef.current) {
        clearTimeout(leaveTimerRef.current);
      }
      unlisten.then((fn) => fn());
    };
  }, [resizeToActive]);

  const handleSessionOpened = useCallback(() => {
    setViewState("collapsed");
    if (!DEBUG_FIXED_WINDOW) {
      resizeToHoverZone().catch((err) =>
        console.error("[notchai-ui] collapse after open failed", err)
      );
    }
  }, [resizeToHoverZone]);

  return (
    <div
      className={`notch-root notch-root--${viewState} notch-root--debug`}
    >
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
                debugLabel={`s:${sessions.length} op:${operatingCount} h:${hoverEnterCount}/${hoverLeaveCount} ${viewState} ${lastHoverEvent}${error ? " err" : ""}`}
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
