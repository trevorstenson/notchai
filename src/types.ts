export type AgentType = "claude" | "codex" | "cursor";

export type AgentStatus =
  | "operating"
  | "idle"
  | "waitingForInput"
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

export const STATUS_COLORS: Record<AgentStatus, string> = {
  operating: "#00FF88",
  idle: "#FFB800",
  waitingForInput: "#7B61FF",
  error: "#FF4444",
  completed: "#555555",
};

export const STATUS_LABELS: Record<AgentStatus, string> = {
  operating: "Operating",
  idle: "Idle",
  waitingForInput: "Needs action",
  error: "Error",
  completed: "Done",
};
