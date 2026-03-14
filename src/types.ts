export type AgentType = "claude" | "codex" | "cursor" | "gemini";

export type AgentStatus =
  | "operating"
  | "idle"
  | "waitingForInput"
  | "waitingForApproval"
  | "error"
  | "completed";

export interface AgentSession {
  agentType: AgentType;
  id: string;
  projectPath: string;
  projectName: string;
  sessionFolderPath: string;
  sessionFolderName: string;
  gitBranch: string;
  firstPrompt: string;
  summary: string | null;
  created: string;
  modified: string;
  status: AgentStatus;
  messageCount: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  currentTask: string | null;
  model: string | null;
  isSidechain: boolean;
}

export interface NotchInfo {
  exists: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  screenWidth: number;
  screenHeight: number;
}

export interface ScreenInfo {
  index: number;
  name: string;
  hasNotch: boolean;
  width: number;
  height: number;
  isPrimary: boolean;
}

export interface ToolCallInfo {
  id: string;
  toolName: string;
  displayName: string;
  inputSummary: string;
  status: string;
  timestamp: string | null;
  durationMs: number | null;
  resultPreview: string | null;
}

export type EventSource = "otel" | "hook" | "notify" | "poll";

export type NormalizedEvent =
  | {
      type: "sessionStarted";
      agentType: AgentType;
      sessionId: string;
      timestamp: string;
      source: EventSource;
    }
  | {
      type: "sessionEnded";
      agentType: AgentType;
      sessionId: string;
      timestamp: string;
      source: EventSource;
    }
  | {
      type: "toolStarted";
      agentType: AgentType;
      sessionId: string;
      timestamp: string;
      source: EventSource;
      toolName: string;
      toolInput: string | null;
    }
  | {
      type: "toolCompleted";
      agentType: AgentType;
      sessionId: string;
      timestamp: string;
      source: EventSource;
      toolName: string;
      status: string;
      durationMs: number | null;
      resultPreview: string | null;
    }
  | {
      type: "statusChanged";
      agentType: AgentType;
      sessionId: string;
      timestamp: string;
      source: EventSource;
      newStatus: AgentStatus;
    }
  | {
      type: "tokensUsed";
      agentType: AgentType;
      sessionId: string;
      timestamp: string;
      source: EventSource;
      inputTokens: number;
      outputTokens: number;
    }
  | {
      type: "permissionRequested";
      agentType: AgentType;
      sessionId: string;
      timestamp: string;
      source: EventSource;
      toolName: string;
      toolInput: string | null;
      requestId: string;
    }
  | {
      type: "taskCompleted";
      agentType: AgentType;
      sessionId: string;
      timestamp: string;
      source: EventSource;
    }
  | {
      type: "error";
      agentType: AgentType;
      sessionId: string;
      timestamp: string;
      source: EventSource;
      message: string;
    };

export const AGENT_COLORS: Record<AgentType, string> = {
  claude: "#E07A40",
  codex: "#00C853",
  cursor: "#4A9FFF",
  gemini: "#A87FFF",
};

export const STATUS_COLORS: Record<AgentStatus, string> = {
  operating: "#00FF88",
  idle: "#FFB800",
  waitingForInput: "#7B61FF",
  waitingForApproval: "#FF8C00",
  error: "#FF4444",
  completed: "#555555",
};

export const STATUS_PRIORITY: Record<AgentStatus, number> = {
  waitingForApproval: 6,
  waitingForInput: 5,
  error: 4,
  operating: 3,
  idle: 2,
  completed: 1,
};

export const STATUS_LABELS: Record<AgentStatus, string> = {
  operating: "Operating",
  idle: "Idle",
  waitingForInput: "Needs action",
  waitingForApproval: "Needs approval",
  error: "Error",
  completed: "Done",
};
