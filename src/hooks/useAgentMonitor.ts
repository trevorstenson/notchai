import { useState, useEffect, useCallback, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AgentSession, NotchInfo } from "../types";
import { useHookEvents, mergeHookStatus } from "./useHookEvents";

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

  // Merge hook-derived status into polled sessions
  const mergedSessions = sessions.map((s) => {
    if (s.agentType !== "claude") return s;
    const hookState = hookStates.get(s.id);
    const mergedStatus = mergeHookStatus(s.status, hookState);
    if (mergedStatus === s.status) return s;
    return { ...s, status: mergedStatus };
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
