import { useState, useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import type { AgentStatus, EventSource, NormalizedEvent } from "../types";

export interface EventBusSessionState {
  status: AgentStatus;
  timestamp: number;
  source: EventSource;
}

/** Maps NormalizedEvent variants to AgentStatus. */
function eventToStatus(event: NormalizedEvent): AgentStatus | null {
  switch (event.type) {
    case "toolStarted":
    case "toolCompleted":
      return "operating";
    case "statusChanged":
      return event.newStatus;
    case "sessionStarted":
      return "operating";
    case "sessionEnded":
    case "taskCompleted":
      return "completed";
    case "error":
      return "error";
    case "tokensUsed":
    case "permissionRequested":
      return null;
  }
}

export function useEventBus() {
  const [eventBusStates, setEventBusStates] = useState<
    Map<string, EventBusSessionState>
  >(() => new Map());

  const onSessionStartRef = useRef<(() => void) | null>(null);
  const bufferRef = useRef<Map<string, EventBusSessionState>>(new Map());
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const unlisten = listen<NormalizedEvent>(
      "event-bus:normalized-event",
      (event) => {
        const normalized = event.payload;
        const status = eventToStatus(normalized);
        if (!status) return;

        const entry: EventBusSessionState = {
          status,
          timestamp: new Date(normalized.timestamp).getTime(),
          source: normalized.source,
        };

        // sessionStarted triggers immediate callback (not debounced)
        if (normalized.type === "sessionStarted" && onSessionStartRef.current) {
          onSessionStartRef.current();
        }

        // Buffer the state update and flush after 150ms
        bufferRef.current.set(normalized.sessionId, entry);
        if (!timerRef.current) {
          timerRef.current = setTimeout(() => {
            const buffered = bufferRef.current;
            if (buffered.size > 0) {
              setEventBusStates((prev) => {
                const next = new Map(prev);
                for (const [k, v] of buffered) next.set(k, v);
                return next;
              });
              buffered.clear();
            }
            timerRef.current = null;
          }, 150);
        }
      },
    );

    return () => {
      unlisten.then((fn) => fn());
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }
    };
  }, []);

  const setOnSessionStart = useCallback((cb: (() => void) | null) => {
    onSessionStartRef.current = cb;
  }, []);

  return { eventBusStates, setOnSessionStart };
}

/**
 * Merge event-bus-sourced status into polled status.
 * Event-bus status takes priority when the event is < 30 seconds old.
 */
export function mergeEventBusStatus(
  polledStatus: AgentStatus,
  ebState: EventBusSessionState | undefined,
): AgentStatus {
  if (!ebState) return polledStatus;

  const age = Date.now() - ebState.timestamp;
  if (age > 30_000) return polledStatus;

  return ebState.status;
}
