import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { StatusDot } from "./StatusDot";
import { STATUS_LABELS } from "../types";
import type { AgentSession } from "../types";

interface ExpandedViewProps {
  sessions: AgentSession[];
  onSessionOpened?: () => void;
}

const RECENT_COMPLETED_WINDOW_MS = 60 * 60 * 1000;
const ROW_FEEDBACK_DURATION_MS = 900;
const DEBUG_MODE = import.meta.env.DEV;

function formatTokens(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
  return String(value);
}

function timeAgo(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  if (diffMins < 1) return "now";
  if (diffMins < 60) return `${diffMins}m`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h`;
  return `${Math.floor(diffHours / 24)}d`;
}

function statusRank(status: AgentSession["status"]): number {
  switch (status) {
    case "operating":
      return 0;
    case "waitingForInput":
      return 1;
    case "idle":
      return 2;
    case "error":
      return 3;
    case "completed":
      return 4;
    default:
      return 5;
  }
}

export function ExpandedView({ sessions, onSessionOpened }: ExpandedViewProps) {
  const [showAllDebug, setShowAllDebug] = useState(false);
  const [openingSessionId, setOpeningSessionId] = useState<string | null>(null);
  const [feedbackBySession, setFeedbackBySession] = useState<
    Record<string, "opened" | "error">
  >({});

  const filteredSessions = useMemo(() => {
    const now = Date.now();

    const meaningful = sessions.filter((session) => {
      const isUnknownCompleted =
        session.status === "completed" && session.projectName === "unknown";

      if (isUnknownCompleted) return false;
      if (session.status !== "completed") return true;

      const modifiedMs = new Date(session.modified).getTime();
      if (Number.isNaN(modifiedMs)) return false;
      return now - modifiedMs <= RECENT_COMPLETED_WINDOW_MS;
    });

    const list = DEBUG_MODE && showAllDebug ? sessions : meaningful;

    return [...list].sort((a, b) => {
      const rankDiff = statusRank(a.status) - statusRank(b.status);
      if (rankDiff !== 0) return rankDiff;
      return new Date(b.modified).getTime() - new Date(a.modified).getTime();
    });
  }, [sessions, showAllDebug]);

  const hiddenCount = sessions.length - filteredSessions.length;

  const openSessionTerminal = async (session: AgentSession) => {
    const targetPath = session.sessionFolderPath || session.projectPath;
    if (!targetPath) return;
    setOpeningSessionId(session.id);
    try {
      if (session.status === "completed") {
        await invoke("resume_session", {
          sessionId: session.id,
          path: targetPath,
        });
      } else {
        await invoke("open_session_location", { path: targetPath });
      }
      setFeedbackBySession((prev) => ({ ...prev, [session.id]: "opened" }));
      setTimeout(() => {
        onSessionOpened?.();
      }, 120);
    } catch (err) {
      console.error("[notchai-ui] open_session_location failed", err);
      setFeedbackBySession((prev) => ({ ...prev, [session.id]: "error" }));
    } finally {
      setOpeningSessionId((current) => (current === session.id ? null : current));
      setTimeout(() => {
        setFeedbackBySession((prev) => {
          const next = { ...prev };
          delete next[session.id];
          return next;
        });
      }, ROW_FEEDBACK_DURATION_MS);
    }
  };

  if (filteredSessions.length === 0) {
    return (
      <div className="expanded-content">
        <div className="expanded-controls">
          <span className="expanded-title">Sessions</span>
          {DEBUG_MODE && (
            <button
              className="expanded-debug-toggle"
              onClick={() => setShowAllDebug((v) => !v)}
            >
              {showAllDebug ? "Hide debug" : "Show all (debug)"}
            </button>
          )}
        </div>
        <div className="expanded-empty">No visible sessions</div>
      </div>
    );
  }

  return (
    <div className="expanded-content">
      <div className="expanded-controls">
        <span className="expanded-title">
          Sessions
          {!showAllDebug && hiddenCount > 0 ? ` (${hiddenCount} hidden)` : ""}
        </span>
        {DEBUG_MODE && (
          <button
            className="expanded-debug-toggle"
            onClick={() => setShowAllDebug((v) => !v)}
          >
            {showAllDebug ? "Hide debug" : "Show all (debug)"}
          </button>
        )}
      </div>
      <div className="expanded-divider" />
      <div className="expanded-sessions">
        {filteredSessions.slice(0, 8).map((session) => {
          const displayProject =
            session.projectName && session.projectName !== "unknown"
              ? session.projectName
              : session.sessionFolderName || "unknown";
          const folderLabel =
            session.sessionFolderName || session.projectPath || "unknown";

          const rowFeedback =
            openingSessionId === session.id
              ? "opening"
              : feedbackBySession[session.id] ?? null;

          return (
            <div
              key={session.id}
              className={`session-row ${
                session.sessionFolderPath || session.projectPath
                  ? "session-row--clickable"
                  : ""
              } ${session.status === "waitingForInput" ? "session-row--waiting" : ""}`}
              onClick={() => openSessionTerminal(session)}
              title={
                session.sessionFolderPath || session.projectPath
                  ? `Open terminal at ${
                      session.sessionFolderPath || session.projectPath
                    }`
                  : "No project path available"
              }
            >
              <div className="session-row-main">
                <StatusDot status={session.status} size={8} />
                <span className="session-name">{displayProject}</span>
                <span className="session-status">
                  {STATUS_LABELS[session.status]}
                </span>
                {rowFeedback && (
                  <span className={`session-action-feedback session-action-feedback--${rowFeedback}`}>
                    {rowFeedback === "opening"
                      ? "Opening…"
                      : rowFeedback === "opened"
                        ? "Opened"
                        : "Failed"}
                  </span>
                )}
                <span className="session-time">{timeAgo(session.modified)}</span>
              </div>

              <div className="session-row-meta">
                <span className="session-meta-item session-meta-folder">
                  folder:{folderLabel}
                </span>
                <span className="session-meta-sep">•</span>
                <span className="session-meta-item">
                  {session.model ?? "unknown model"}
                </span>
                <span className="session-meta-sep">•</span>
                <span className="session-meta-item">
                  {session.gitBranch || "no-branch"}
                </span>
                <span className="session-meta-sep">•</span>
                <span className="session-meta-item">
                  in:{formatTokens(session.totalInputTokens)} out:
                  {formatTokens(session.totalOutputTokens)}
                </span>
              </div>

              <div className="session-row-preview">
                {session.summary || session.firstPrompt || "No summary yet"}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
