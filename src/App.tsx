import { useState, useCallback, useRef } from "react";
import {
  getCurrentWindow,
  LogicalSize,
  LogicalPosition,
} from "@tauri-apps/api/window";
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

// Timing
const EXPAND_DELAY_MS = 300;
const COLLAPSE_DELAY_MS = 200;
const HIDE_DELAY_MS = 300;

function App() {
  const { sessions, activeSessions, operatingCount, notchInfo } =
    useAgentMonitor(2000);
  const [viewState, setViewState] = useState<ViewState>("hidden");

  const expandTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearTimers = useCallback(() => {
    if (expandTimerRef.current) {
      clearTimeout(expandTimerRef.current);
      expandTimerRef.current = null;
    }
    if (hideTimerRef.current) {
      clearTimeout(hideTimerRef.current);
      hideTimerRef.current = null;
    }
  }, []);

  const getCenterX = useCallback(() => {
    return notchInfo ? notchInfo.x + notchInfo.width / 2 : 756;
  }, [notchInfo]);

  const resizeToHoverZone = useCallback(async () => {
    const win = getCurrentWindow();
    const centerX = getCenterX();
    await win.setSize(new LogicalSize(HOVER_ZONE_WIDTH, HOVER_ZONE_HEIGHT));
    await win.setPosition(
      new LogicalPosition(centerX - HOVER_ZONE_WIDTH / 2, 0)
    );
  }, [getCenterX]);

  const resizeToActive = useCallback(async () => {
    const win = getCurrentWindow();
    const centerX = getCenterX();
    await win.setSize(new LogicalSize(ACTIVE_WIDTH, ACTIVE_HEIGHT));
    await win.setPosition(
      new LogicalPosition(centerX - ACTIVE_WIDTH / 2, 0)
    );
  }, [getCenterX]);

  const handleMouseEnter = useCallback(() => {
    clearTimers();

    if (viewState === "hidden") {
      // Resize window to active size, then show island
      resizeToActive().then(() => {
        setViewState("collapsed");
        expandTimerRef.current = setTimeout(() => {
          setViewState("expanded");
        }, EXPAND_DELAY_MS);
      });
    } else if (viewState === "collapsed") {
      // Already active size, just schedule expand
      expandTimerRef.current = setTimeout(() => {
        setViewState("expanded");
      }, EXPAND_DELAY_MS);
    }
    // If already expanded, timers are cleared — stay expanded
  }, [viewState, clearTimers, resizeToActive]);

  const handleMouseLeave = useCallback(() => {
    clearTimers();

    if (viewState === "expanded") {
      hideTimerRef.current = setTimeout(() => {
        setViewState("collapsed");
        hideTimerRef.current = setTimeout(() => {
          setViewState("hidden");
          // Shrink window back after CSS fade-out
          setTimeout(() => resizeToHoverZone(), 150);
        }, HIDE_DELAY_MS);
      }, COLLAPSE_DELAY_MS);
    } else if (viewState === "collapsed") {
      hideTimerRef.current = setTimeout(() => {
        setViewState("hidden");
        setTimeout(() => resizeToHoverZone(), 150);
      }, HIDE_DELAY_MS);
    }
  }, [viewState, clearTimers, resizeToHoverZone]);

  return (
    <div
      className={`notch-root notch-root--${viewState}`}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      <div className="island-wrapper">
        <div className={`island island--${viewState}`}>
          {viewState !== "hidden" && (
            <>
              <CollapsedView
                sessions={activeSessions}
                operatingCount={operatingCount}
              />
              {viewState === "expanded" && (
                <ExpandedView sessions={sessions} />
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
