import { useState, useCallback, useRef } from "react";
import {
  getCurrentWindow,
  LogicalSize,
  LogicalPosition,
} from "@tauri-apps/api/window";
import { useAgentMonitor } from "./hooks/useAgentMonitor";
import { CollapsedView } from "./components/CollapsedView";
import { ExpandedView } from "./components/ExpandedView";
import { DetailView } from "./components/DetailView";
import "./App.css";

type ViewState = "hidden" | "collapsed" | "expanded";

// Hover zone dimensions — matches lib.rs setup
const HOVER_ZONE_WIDTH = 400;
const HOVER_ZONE_HEIGHT = 50;

const EXPANDED_WIDTH = 380;
const EXPANDED_HEIGHT = 520;

// Timing
const EXPAND_DELAY_MS = 200;
const COLLAPSE_DELAY_MS = 300;
const HIDE_DELAY_MS = 400;

function App() {
  const { sessions, activeSessions, operatingCount, notchInfo } =
    useAgentMonitor(2000);
  const [viewState, setViewState] = useState<ViewState>("hidden");
  const [selectedId, setSelectedId] = useState<string | null>(null);

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

  const resizeToExpanded = useCallback(async () => {
    const win = getCurrentWindow();
    const centerX = getCenterX();
    await win.setSize(new LogicalSize(EXPANDED_WIDTH, EXPANDED_HEIGHT));
    await win.setPosition(
      new LogicalPosition(centerX - EXPANDED_WIDTH / 2, 0)
    );
  }, [getCenterX]);

  const handleMouseEnter = useCallback(() => {
    clearTimers();

    if (viewState === "hidden") {
      setViewState("collapsed");
      expandTimerRef.current = setTimeout(async () => {
        // Resize window FIRST so content isn't clipped
        await resizeToExpanded();
        setViewState("expanded");
      }, EXPAND_DELAY_MS);
    } else if (viewState === "collapsed") {
      expandTimerRef.current = setTimeout(async () => {
        await resizeToExpanded();
        setViewState("expanded");
      }, EXPAND_DELAY_MS);
    }
    // If already expanded, timers are cleared — stay expanded
  }, [viewState, clearTimers, resizeToExpanded]);

  const handleMouseLeave = useCallback(() => {
    clearTimers();

    if (viewState === "expanded") {
      hideTimerRef.current = setTimeout(async () => {
        // Remove content FIRST, then shrink window
        setViewState("collapsed");
        setSelectedId(null);
        await resizeToHoverZone();

        hideTimerRef.current = setTimeout(() => {
          setViewState("hidden");
        }, HIDE_DELAY_MS);
      }, COLLAPSE_DELAY_MS);
    } else if (viewState === "collapsed") {
      hideTimerRef.current = setTimeout(() => {
        setViewState("hidden");
      }, HIDE_DELAY_MS);
    }
  }, [viewState, clearTimers, resizeToHoverZone]);

  const selectedSession = sessions.find((s) => s.id === selectedId) || null;

  return (
    <div
      className={`notch-root notch-root--${viewState}`}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      {viewState !== "hidden" && (
        <div className="notch-container">
          <CollapsedView
            sessions={activeSessions}
            operatingCount={operatingCount}
          />

          {viewState === "expanded" && (
            <div className="expanded-container">
              <ExpandedView
                sessions={sessions}
                selectedId={selectedId}
                onSelect={setSelectedId}
              />

              {selectedSession && (
                <DetailView
                  session={selectedSession}
                  onClose={() => setSelectedId(null)}
                />
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default App;
