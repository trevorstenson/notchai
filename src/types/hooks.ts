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
  isQuestion: boolean;
  permissionSuggestions: string | null;
}

export interface HookSessionState {
  sessionId: string;
  lastEventType: string;
  lastTimestamp: string;
  pendingApproval: PermissionRequestEvent | null;
}

// AskUserQuestion tool input schema
export interface AskUserQuestionOption {
  label: string;
  description: string;
  markdown?: string;
}

export interface AskUserQuestionItem {
  question: string;
  header: string;
  options: AskUserQuestionOption[];
  multiSelect: boolean;
}

export interface AskUserQuestionInput {
  questions: AskUserQuestionItem[];
  answers?: Record<string, string>;
}

/** Parse tool_input string as AskUserQuestion. Returns null on failure. */
export function parseAskUserQuestion(
  toolInput: string | null,
): AskUserQuestionInput | null {
  if (!toolInput) return null;
  try {
    const parsed = JSON.parse(toolInput);
    if (
      parsed &&
      Array.isArray(parsed.questions) &&
      parsed.questions.length > 0 &&
      parsed.questions.every(
        (q: Record<string, unknown>) =>
          typeof q === "object" &&
          q !== null &&
          "question" in q &&
          "options" in q &&
          Array.isArray(q.options),
      )
    ) {
      return parsed as AskUserQuestionInput;
    }
  } catch {
    // Parsing failed, fall back to generic card
  }
  return null;
}
