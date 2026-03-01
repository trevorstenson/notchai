import { useState, useEffect, useCallback, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AgentSession, NotchInfo } from "../types";
import { useHookEvents, mergeHookStatus } from "./useHookEvents";
import { useEventBus, mergeEventBusStatus } from "./useEventBus";

export function useAgentMonitor(
  pollIntervalMs = 3000,
  animatingRef?: MutableRefObject<boolean>,
) {
  const [sessions, setSessions] = useState<AgentSession[]>([]);
  const [notchInfo, setNotchInfo] = useState<NotchInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  const {
    hookStates,
    pendingApprovals,
    respondToApproval,
    setOnSessionStart,
  } = useHookEvents();

  const { eventBusStates } = useEventBus();

  const fetchSessions = useCallback(async () => {
    // Skip updates during animation to avoid mid-animation re-renders
    if (animatingRef?.current) return;
    try {
      const result = await invoke<AgentSession[]>("get_sessions");
      setSessions(result);
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  }, [animatingRef]);

  const fetchNotchInfo = useCallback(async () => {
    try {
      const result = await invoke<NotchInfo>("get_notch_info");
      setNotchInfo(result);
    } catch (err) {
      console.error("Failed to get notch info:", err);
    }
  }, []);

  // Register SessionStart callback to trigger immediate poll refresh
  useEffect(() => {
    setOnSessionStart(() => {
      fetchSessions();
    });
    return () => setOnSessionStart(null);
  }, [setOnSessionStart, fetchSessions]);

  useEffect(() => {
    fetchNotchInfo();
    fetchSessions();

    const interval = setInterval(fetchSessions, pollIntervalMs);
    return () => clearInterval(interval);
  }, [fetchSessions, fetchNotchInfo, pollIntervalMs]);

  // Merge hook-derived and event-bus status into polled sessions
  // Priority: event-bus > hooks > poll
  const mergedSessions = sessions.map((s) => {
    let status = s.status;

    // First layer: hook status (claude-only, same as before)
    if (s.agentType === "claude") {
      const hookState = hookStates.get(s.id);
      status = mergeHookStatus(status, hookState);
    }

    // Second layer: event-bus status (all agent types, highest priority)
    const ebState = eventBusStates.get(s.id);
    status = mergeEventBusStatus(status, ebState);

    if (status === s.status) return s;
    return { ...s, status };
  });

  const activeSessions = mergedSessions.filter((s) => s.status !== "completed");
  const operatingCount = mergedSessions.filter(
    (s) => s.status === "operating"
  ).length;

  return {
    sessions: mergedSessions,
    activeSessions,
    operatingCount,
    notchInfo,
    error,
    refresh: fetchSessions,
    pendingApprovals,
    respondToApproval,
  };
}
