import { useState, useEffect, useCallback, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AgentSession, NotchInfo } from "../types";

export function useAgentMonitor(
  pollIntervalMs = 2000,
  animatingRef?: MutableRefObject<boolean>,
) {
  const [sessions, setSessions] = useState<AgentSession[]>([]);
  const [notchInfo, setNotchInfo] = useState<NotchInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

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

  useEffect(() => {
    fetchNotchInfo();
    fetchSessions();

    const interval = setInterval(fetchSessions, pollIntervalMs);
    return () => clearInterval(interval);
  }, [fetchSessions, fetchNotchInfo, pollIntervalMs]);

  const activeSessions = sessions.filter((s) => s.status !== "completed");
  const operatingCount = sessions.filter(
    (s) => s.status === "operating"
  ).length;

  return {
    sessions,
    activeSessions,
    operatingCount,
    notchInfo,
    error,
    refresh: fetchSessions,
  };
}
