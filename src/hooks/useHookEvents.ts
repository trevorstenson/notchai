import { useState, useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type {
  HookStatusEvent,
  PermissionRequestEvent,
  HookSessionState,
} from "../types/hooks";
import type { AgentStatus } from "../types";

/** Maps hook event types to AgentStatus values. */
function hookEventToStatus(eventType: string): AgentStatus | null {
  switch (eventType) {
    case "UserPromptSubmit":
    case "PreToolUse":
    case "PostToolUse":
      return "operating";
    case "Stop":
    case "SubagentStop":
    case "SessionEnd":
      return "completed";
    case "PermissionRequest":
      return "waitingForApproval";
    default:
      return null;
  }
}

export function useHookEvents() {
  const [hookStates, setHookStates] = useState<Map<string, HookSessionState>>(
    () => new Map(),
  );
  const [pendingApprovals, setPendingApprovals] = useState<
    PermissionRequestEvent[]
  >([]);

  // Track a callback that the consumer can set for "SessionStart triggers refresh"
  const onSessionStartRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    const unlistenStatus = listen<HookStatusEvent>(
      "hook:status-update",
      (event) => {
        const payload = event.payload;
        setHookStates((prev) => {
          const next = new Map(prev);
          next.set(payload.sessionId, {
            sessionId: payload.sessionId,
            lastEventType: payload.eventType,
            lastTimestamp: payload.timestamp,
            pendingApproval: null,
          });
          return next;
        });

        // SessionStart triggers an immediate poll refresh
        if (
          payload.eventType === "SessionStart" &&
          onSessionStartRef.current
        ) {
          onSessionStartRef.current();
        }
      },
    );

    const unlistenPermission = listen<PermissionRequestEvent>(
      "hook:permission-request",
      (event) => {
        const payload = event.payload;
        setHookStates((prev) => {
          const next = new Map(prev);
          next.set(payload.sessionId, {
            sessionId: payload.sessionId,
            lastEventType: "PermissionRequest",
            lastTimestamp: payload.timestamp,
            pendingApproval: payload,
          });
          return next;
        });
        setPendingApprovals((prev) => [...prev, payload]);
      },
    );

    return () => {
      unlistenStatus.then((fn) => fn());
      unlistenPermission.then((fn) => fn());
    };
  }, []);

  const respondToApproval = useCallback(
    async (requestId: string, decision: string, reason?: string) => {
      await invoke("respond_to_approval", {
        requestId,
        decision,
        reason: reason ?? null,
      });

      // Remove from pending list
      setPendingApprovals((prev) =>
        prev.filter((p) => p.requestId !== requestId),
      );

      // Clear the pendingApproval from the hook state
      setHookStates((prev) => {
        const next = new Map(prev);
        for (const [sid, state] of next) {
          if (state.pendingApproval?.requestId === requestId) {
            next.set(sid, { ...state, pendingApproval: null });
          }
        }
        return next;
      });
    },
    [],
  );

  /** Register a callback to fire when a SessionStart event arrives. */
  const setOnSessionStart = useCallback((cb: (() => void) | null) => {
    onSessionStartRef.current = cb;
  }, []);

  return {
    hookStates,
    pendingApprovals,
    respondToApproval,
    setOnSessionStart,
  };
}

/**
 * Given a session ID, determine the effective status by merging
 * hook-derived state with the polled status.
 * Hook state takes priority when it's less than 30 seconds old.
 */
export function mergeHookStatus(
  polledStatus: AgentStatus,
  hookState: HookSessionState | undefined,
): AgentStatus {
  if (!hookState) return polledStatus;

  const hookAge = Date.now() - new Date(hookState.lastTimestamp).getTime();
  if (hookAge > 30_000) return polledStatus;

  const hookStatus = hookEventToStatus(hookState.lastEventType);
  if (!hookStatus) return polledStatus;

  return hookStatus;
}
