import { useState, useEffect } from "react";
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

  useEffect(() => {
    const unlisten = listen<NormalizedEvent>(
      "event-bus:normalized-event",
      (event) => {
        const normalized = event.payload;
        const status = eventToStatus(normalized);
        if (!status) return;

        setEventBusStates((prev) => {
          const next = new Map(prev);
          next.set(normalized.sessionId, {
            status,
            timestamp: new Date(normalized.timestamp).getTime(),
            source: normalized.source,
          });
          return next;
        });
      },
    );

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return { eventBusStates };
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
