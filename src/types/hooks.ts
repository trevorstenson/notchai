export interface HookStatusEvent {
  eventType: string;
  sessionId: string;
  cwd: string | null;
  toolName: string | null;
  agent: string | null;
  timestamp: string;
}

export interface PermissionRequestEvent {
  requestId: string;
  sessionId: string;
  toolName: string;
  toolInput: string | null;
  cwd: string | null;
  agent: string | null;
  timestamp: string;
}

export interface HookSessionState {
  sessionId: string;
  lastEventType: string;
  lastTimestamp: string;
  pendingApproval: PermissionRequestEvent | null;
}
