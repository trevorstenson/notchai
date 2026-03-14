import { useMemo, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PermissionRequestEvent } from "../types/hooks";
import { parseAskUserQuestion } from "../types/hooks";
import { QuestionCard } from "./QuestionCard";

interface ToolApprovalProps {
  pendingApprovals: PermissionRequestEvent[];
  respondToApproval: (
    requestId: string,
    decision: string,
    reason?: string,
    updatedInput?: string,
    updatedPermissions?: string,
  ) => Promise<void>;
}

/** Tool icons by tool name */
const TOOL_ICONS: Record<string, string> = {
  Bash: "\u25B6",        // ▶
  Edit: "\u270E",        // ✎
  Write: "\u2710",       // ✐
  Read: "\u25A1",        // □
  Glob: "\u2026",        // …
  Grep: "\u2315",        // ⌕
  WebFetch: "\u2197",    // ↗
  WebSearch: "\u2315",   // ⌕
  Agent: "\u2699",       // ⚙
};

interface ParsedToolDisplay {
  headline: string | null;
  detail: string | null;
}

/** Extract a human-readable headline + detail from tool_input JSON. */
function parseToolDisplay(
  toolName: string,
  toolInput: string | null,
): ParsedToolDisplay {
  if (!toolInput) return { headline: null, detail: null };
  try {
    const parsed = JSON.parse(toolInput);
    switch (toolName) {
      case "Bash":
        return {
          headline: parsed.description ?? null,
          detail: parsed.command ?? null,
        };
      case "Edit":
        return {
          headline: parsed.file_path ?? null,
          detail: parsed.old_string
            ? `Replace: ${truncate(parsed.old_string, 80)}`
            : null,
        };
      case "Write":
        return {
          headline: parsed.file_path ?? null,
          detail: parsed.content ? `${parsed.content.length} chars` : null,
        };
      case "Read":
        return {
          headline: parsed.file_path ?? null,
          detail:
            parsed.offset || parsed.limit
              ? `lines ${parsed.offset ?? 0}–${(parsed.offset ?? 0) + (parsed.limit ?? "end")}`
              : null,
        };
      case "Glob":
        return {
          headline: parsed.pattern ?? null,
          detail: parsed.path ?? null,
        };
      case "Grep":
        return {
          headline: parsed.pattern ?? null,
          detail: parsed.path ?? parsed.glob ?? null,
        };
      case "WebFetch":
        return {
          headline: parsed.url ?? null,
          detail: parsed.prompt ? truncate(parsed.prompt, 80) : null,
        };
      case "WebSearch":
        return {
          headline: parsed.query ?? null,
          detail: null,
        };
      case "Agent":
        return {
          headline: parsed.subagent_type
            ? `${parsed.subagent_type}: ${parsed.description ?? ""}`
            : parsed.description ?? null,
          detail: parsed.prompt ? truncate(parsed.prompt, 100) : null,
        };
      default: {
        // MCP tools or unknown: try to show something useful
        const keys = Object.keys(parsed);
        if (keys.length === 0) return { headline: null, detail: null };
        // Show first string value as headline
        for (const k of keys) {
          if (typeof parsed[k] === "string" && parsed[k].length > 0) {
            return {
              headline: `${k}: ${truncate(parsed[k], 80)}`,
              detail: null,
            };
          }
        }
        return { headline: null, detail: truncate(toolInput, 120) };
      }
    }
  } catch {
    // Not valid JSON, show raw (truncated)
    return { headline: null, detail: truncate(toolInput, 120) };
  }
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max - 1) + "\u2026";
}

/** A parsed permission suggestion with a label and the permissions payload to send. */
interface PermissionOption {
  label: string;
  updatedPermissions: string;
}

/** Generate a descriptive label from a rule's content and destination. */
function labelFromRule(
  rule: { ruleContent?: string; toolName?: string },
  destination?: string,
): string {
  const prefix = destination === "session" ? "Session" : "Always";
  const rc = rule.ruleContent ?? "";
  if (rc.startsWith("domain:")) return `${prefix} for ${rc.slice(7)}`;
  if (rc.startsWith("//")) return `${prefix} for ${rc.replace(/^\/\//, "/")}`;
  if (rc.length > 20) return `${prefix} for ${rc.slice(0, 17)}…`;
  if (rc) return `${prefix} for ${rc}`;
  return prefix;
}

/** Parse permission_suggestions into renderable button options.
 *  Supports both legacy `toolAlwaysAllow` and current `addRules` formats. */
function parsePermissionOptions(permissionSuggestions: string | null): PermissionOption[] {
  if (!permissionSuggestions) return [];
  try {
    const suggestions = JSON.parse(permissionSuggestions);
    if (!Array.isArray(suggestions)) return [];

    const options: PermissionOption[] = [];
    for (const s of suggestions) {
      if (s.type === "toolAlwaysAllow") {
        // Legacy format: { type: "toolAlwaysAllow", tool: "Bash" }
        options.push({
          label: `Always Allow`,
          updatedPermissions: JSON.stringify([{ pattern: s.tool, allow: true }]),
        });
      } else if (s.type === "addRules" && Array.isArray(s.rules)) {
        // Current format: { type: "addRules", rules: [...], behavior, destination }
        const dest = s.destination as string | undefined;
        // Use the first rule to generate a descriptive label
        const firstRule = s.rules[0] as { ruleContent?: string; toolName?: string } | undefined;
        const label = firstRule
          ? labelFromRule(firstRule, dest)
          : dest === "session" ? "Allow Session" : "Always Allow";
        options.push({
          label,
          updatedPermissions: JSON.stringify(s.rules),
        });
      }
    }
    return options;
  } catch {
    return [];
  }
}

export function ToolApproval({
  pendingApprovals,
  respondToApproval,
}: ToolApprovalProps) {
  // Play sound + haptic when a new approval appears
  const lastApprovalId = useRef<string | null>(null);
  useEffect(() => {
    if (pendingApprovals.length === 0) return;
    const currentId = pendingApprovals[0].requestId;
    if (currentId !== lastApprovalId.current) {
      lastApprovalId.current = currentId;
      invoke<boolean>("get_sound_enabled")
        .then((enabled) => {
          if (enabled) {
            invoke("play_sound", { name: "Tink" }).catch(() => {});
            invoke("play_haptic").catch(() => {});
          }
        })
        .catch(() => {});
    }
  }, [pendingApprovals]);

  if (pendingApprovals.length === 0) return null;

  const current = pendingApprovals[0];
  const remaining = pendingApprovals.length - 1;

  // AskUserQuestion: render interactive question card
  if (current.isQuestion && parseAskUserQuestion(current.toolInput)) {
    return (
      <QuestionCard
        approval={current}
        remaining={remaining}
        respondToApproval={respondToApproval}
      />
    );
  }

  const icon = TOOL_ICONS[current.toolName] ?? "\u26A1"; // ⚡ default
  const projectContext = current.cwd
    ? current.cwd.split("/").pop() || current.cwd
    : null;

  return (
    <div className="tool-approval">
      <div className="tool-approval-card">
        <div className="tool-approval-header">
          <span className="tool-approval-icon">{icon}</span>
          <span className="tool-approval-title">Tool Approval</span>
          {remaining > 0 && (
            <span className="tool-approval-badge">+{remaining} more</span>
          )}
        </div>

        <ToolBody
          toolName={current.toolName}
          toolInput={current.toolInput}
          projectContext={projectContext}
        />

        <ApprovalActions
          requestId={current.requestId}
          permissionSuggestions={current.permissionSuggestions}
          respondToApproval={respondToApproval}
        />
      </div>
    </div>
  );
}

/** Render approval action buttons dynamically based on permission_suggestions. */
function ApprovalActions({
  requestId,
  permissionSuggestions,
  respondToApproval,
}: {
  requestId: string;
  permissionSuggestions: string | null;
  respondToApproval: ToolApprovalProps["respondToApproval"];
}) {
  const options = useMemo(
    () => parsePermissionOptions(permissionSuggestions),
    [permissionSuggestions],
  );

  return (
    <div className="tool-approval-actions">
      <button
        className="tool-approval-btn tool-approval-btn--deny"
        onClick={() =>
          respondToApproval(requestId, "deny", "Denied by user in Notchai")
        }
      >
        Deny
      </button>
      {options.map((opt, i) => (
        <button
          key={i}
          className="tool-approval-btn tool-approval-btn--always"
          onClick={() =>
            respondToApproval(
              requestId,
              "allow",
              undefined,
              undefined,
              opt.updatedPermissions,
            )
          }
        >
          {opt.label}
        </button>
      ))}
      <button
        className="tool-approval-btn tool-approval-btn--allow"
        onClick={() => respondToApproval(requestId, "allow")}
      >
        Allow
      </button>
    </div>
  );
}

function ToolBody({
  toolName,
  toolInput,
  projectContext,
}: {
  toolName: string;
  toolInput: string | null;
  projectContext: string | null;
}) {
  const display = useMemo(
    () => parseToolDisplay(toolName, toolInput),
    [toolName, toolInput],
  );

  return (
    <div className="tool-approval-body">
      <div className="tool-approval-tool-name">{toolName}</div>
      {display.headline && (
        <div className="tool-approval-headline">{display.headline}</div>
      )}
      {display.detail && (
        <div className="tool-approval-detail">{display.detail}</div>
      )}
      {!display.headline && !display.detail && toolInput && (
        <div className="tool-approval-input">{toolInput}</div>
      )}
      {projectContext && (
        <div className="tool-approval-context">{projectContext}</div>
      )}
    </div>
  );
}
