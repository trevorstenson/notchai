import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ToolCallInfo } from "../types";

export function useSessionToolCalls(sessionId: string | null) {
  const [toolCalls, setToolCalls] = useState<ToolCallInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchToolCalls = useCallback(async (id: string) => {
    try {
      const result = await invoke<ToolCallInfo[]>("get_session_tool_calls", {
        sessionId: id,
      });
      setToolCalls(result);
    } catch (err) {
      console.error("[notchai-ui] get_session_tool_calls failed", err);
    }
  }, []);

  useEffect(() => {
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }

    if (!sessionId) {
      setToolCalls([]);
      setLoading(false);
      return;
    }

    setLoading(true);
    fetchToolCalls(sessionId).then(() => setLoading(false));

    intervalRef.current = setInterval(() => {
      fetchToolCalls(sessionId);
    }, 3000);

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [sessionId, fetchToolCalls]);

  return { toolCalls, loading };
}
