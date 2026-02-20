import { useState, useCallback } from "react";
import { getCurrentWindow, LogicalSize, LogicalPosition } from "@tauri-apps/api/window";
import { useAgentMonitor } from "./hooks/useAgentMonitor";
import { CollapsedView } from "./components/CollapsedView";
import { ExpandedView } from "./components/ExpandedView";
import { DetailView } from "./components/DetailView";
import "./App.css";

const COLLAPSED_WIDTH = 280;
const COLLAPSED_HEIGHT = 40;
const EXPANDED_WIDTH = 380;
const EXPANDED_HEIGHT = 520;

function App() {
  const { sessions, activeSessions, operatingCount, notchInfo } =
    useAgentMonitor(2000);
  const [isExpanded, setIsExpanded] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const expand = useCallback(async () => {
    if (isExpanded) return;
    setIsExpanded(true);
    const win = getCurrentWindow();
    const centerX = notchInfo
      ? notchInfo.x + notchInfo.width / 2
      : 756;
    await win.setSize(new LogicalSize(EXPANDED_WIDTH, EXPANDED_HEIGHT));
    await win.setPosition(
      new LogicalPosition(centerX - EXPANDED_WIDTH / 2, 0)
    );
  }, [isExpanded, notchInfo]);

  const collapse = useCallback(async () => {
    if (!isExpanded) return;
    setIsExpanded(false);
    setSelectedId(null);
    const win = getCurrentWindow();
    const centerX = notchInfo
      ? notchInfo.x + notchInfo.width / 2
      : 756;
    await win.setSize(new LogicalSize(COLLAPSED_WIDTH, COLLAPSED_HEIGHT));
    await win.setPosition(
      new LogicalPosition(centerX - COLLAPSED_WIDTH / 2, 0)
    );
  }, [isExpanded, notchInfo]);

  const selectedSession = sessions.find((s) => s.id === selectedId) || null;

  return (
    <div
      className="notch-root"
      onMouseEnter={expand}
      onMouseLeave={collapse}
    >
      <div className="notch-container">
        <CollapsedView
          sessions={activeSessions}
          operatingCount={operatingCount}
        />

        {isExpanded && (
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
    </div>
  );
}

export default App;
