import { useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import type { AgentSession, AgentStatus } from "../types";

const NOTIFY_STATUSES: Set<AgentStatus> = new Set([
  "waitingForInput",
  "completed",
]);

const ACTIVE_STATUSES: Set<AgentStatus> = new Set(["operating", "idle"]);

export function useSessionNotifications(sessions: AgentSession[]) {
  const prevStatusMap = useRef<Map<string, AgentStatus>>(new Map());
  const permissionGranted = useRef<boolean | null>(null);
  const initialLoad = useRef(true);

  useEffect(() => {
    (async () => {
      let granted = await isPermissionGranted();
      if (!granted) {
        const result = await requestPermission();
        granted = result === "granted";
      }
      permissionGranted.current = granted;
    })();
  }, []);

  useEffect(() => {
    if (initialLoad.current) {
      const map = new Map<string, AgentStatus>();
      for (const session of sessions) {
        map.set(session.id, session.status);
      }
      prevStatusMap.current = map;
      initialLoad.current = false;
      return;
    }

    if (!permissionGranted.current) return;

    const prevMap = prevStatusMap.current;
    const nextMap = new Map<string, AgentStatus>();

    for (const session of sessions) {
      nextMap.set(session.id, session.status);

      const prevStatus = prevMap.get(session.id);
      if (
        prevStatus !== undefined &&
        prevStatus !== session.status &&
        NOTIFY_STATUSES.has(session.status) &&
        ACTIVE_STATUSES.has(prevStatus)
      ) {
        fireNotification(session);
      }
    }

    prevStatusMap.current = nextMap;
  }, [sessions]);
}

function playFeedback(soundName: string) {
  invoke<boolean>("get_sound_enabled")
    .then((enabled) => {
      if (enabled) {
        invoke("play_sound", { name: soundName }).catch(() => {});
        invoke("play_haptic").catch(() => {});
      }
    })
    .catch(() => {});
}

function fireNotification(session: AgentSession) {
  const projectLabel =
    session.projectName && session.projectName !== "unknown"
      ? session.projectName
      : session.sessionFolderName || "Session";

  if (session.status === "waitingForInput") {
    sendNotification({
      title: `${projectLabel} needs input`,
      body:
        session.summary ||
        session.firstPrompt ||
        "Agent is waiting for your response.",
    });
    playFeedback("Purr");
  } else if (session.status === "completed") {
    sendNotification({
      title: `${projectLabel} completed`,
      body:
        session.summary ||
        session.firstPrompt ||
        "Agent session has finished.",
    });
    playFeedback("Pop");
  }
}
