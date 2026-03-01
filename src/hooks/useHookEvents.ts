import { useState, useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type {
  HookStatusEvent,
  PermissionRequestEvent,
  HookSessionState,
  NotificationEvent,
} from "../types/hooks";
import type { AgentStatus } from "../types";

/** Maps hook event types to AgentStatus values. */
function hookEventToStatus(eventType: string): AgentStatus | null {
  switch (eventType) {
    case "UserPromptSubmit":
    case "PreToolUse":
    case "PostToolUse":
    case "SessionStart":
    case "Notification":
      return "operating";
    case "Stop":
    case "SubagentStop":
    case "SessionEnd":
      return "completed";
    case "PermissionRequest":
      return "waitingForApproval";
    default:
      // Unknown/new event types treated as operating (agent is active)
      return "operating";
  }
}

/** Duration (ms) to show notification text in collapsed view before auto-clearing. */
const NOTIFICATION_DISPLAY_MS = 4000;

/** TTL (ms) for pending approvals — matches backend APPROVAL_TTL_SECS (5 minutes). */
const APPROVAL_TTL_MS = 5 * 60 * 1000;

/** Filter out pending approvals older than APPROVAL_TTL_MS. */
function filterStaleApprovals(
  approvals: PermissionRequestEvent[],
): PermissionRequestEvent[] {
  const cutoff = Date.now() - APPROVAL_TTL_MS;
  return approvals.filter(
    (a) => new Date(a.timestamp).getTime() > cutoff,
  );
}

export function useHookEvents() {
  const [hookStates, setHookStates] = useState<Map<string, HookSessionState>>(
    () => new Map(),
  );
  const [pendingApprovals, setPendingApprovals] = useState<
    PermissionRequestEvent[]
  >([]);
  const [notificationText, setNotificationText] = useState<string | null>(null);
  const notificationTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );

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
        setPendingApprovals((prev) =>
          filterStaleApprovals([...prev, payload]),
        );
      },
    );

    // Listen for approvals that were handled elsewhere (e.g., user approved in terminal)
    const unlistenCancelled = listen<string>(
      "hook:permission-cancelled",
      (event) => {
        const cancelledId = event.payload;
        setPendingApprovals((prev) =>
          filterStaleApprovals(
            prev.filter((p) => p.requestId !== cancelledId),
          ),
        );
        setHookStates((prev) => {
          const next = new Map(prev);
          for (const [sid, state] of next) {
            if (state.pendingApproval?.requestId === cancelledId) {
              next.set(sid, { ...state, pendingApproval: null });
            }
          }
          return next;
        });
      },
    );

    // Listen for Notification events to show brief text in collapsed view
    const unlistenNotification = listen<NotificationEvent>(
      "hook:notification",
      (event) => {
        const { title } = event.payload;
        setNotificationText(title);
        // Clear any existing timer
        if (notificationTimerRef.current) {
          clearTimeout(notificationTimerRef.current);
        }
        notificationTimerRef.current = setTimeout(() => {
          setNotificationText(null);
          notificationTimerRef.current = null;
        }, NOTIFICATION_DISPLAY_MS);
      },
    );

    return () => {
      unlistenStatus.then((fn) => fn());
      unlistenPermission.then((fn) => fn());
      unlistenCancelled.then((fn) => fn());
      unlistenNotification.then((fn) => fn());
      if (notificationTimerRef.current) {
        clearTimeout(notificationTimerRef.current);
      }
    };
  }, []);

  const respondToApproval = useCallback(
    async (
      requestId: string,
      decision: string,
      reason?: string,
      updatedInput?: string,
      updatedPermissions?: string,
    ) => {
      await invoke("respond_to_approval", {
        requestId,
        decision,
        reason: reason ?? null,
        updatedInput: updatedInput ?? null,
        updatedPermissions: updatedPermissions ?? null,
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
    notificationText,
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
