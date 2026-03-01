import type { PermissionRequestEvent } from "../types/hooks";

interface ToolApprovalProps {
  pendingApprovals: PermissionRequestEvent[];
  respondToApproval: (
    requestId: string,
    decision: string,
    reason?: string,
  ) => Promise<void>;
}

export function ToolApproval({
  pendingApprovals,
  respondToApproval,
}: ToolApprovalProps) {
  if (pendingApprovals.length === 0) return null;

  const current = pendingApprovals[0];
  const remaining = pendingApprovals.length - 1;

  const projectContext = current.cwd
    ? current.cwd.split("/").pop() || current.cwd
    : null;

  return (
    <div className="tool-approval">
      <div className="tool-approval-card">
        <div className="tool-approval-header">
          <span className="tool-approval-icon">⚡</span>
          <span className="tool-approval-title">Tool Approval</span>
          {remaining > 0 && (
            <span className="tool-approval-badge">+{remaining} more</span>
          )}
        </div>

        <div className="tool-approval-body">
          <div className="tool-approval-tool-name">{current.toolName}</div>
          {current.toolInput && (
            <div className="tool-approval-input">{current.toolInput}</div>
          )}
          {projectContext && (
            <div className="tool-approval-context">{projectContext}</div>
          )}
        </div>

        <div className="tool-approval-actions">
          <button
            className="tool-approval-btn tool-approval-btn--deny"
            onClick={() =>
              respondToApproval(
                current.requestId,
                "deny",
                "Denied by user in Notchai",
              )
            }
          >
            Deny
          </button>
          <button
            className="tool-approval-btn tool-approval-btn--allow"
            onClick={() => respondToApproval(current.requestId, "allow")}
          >
            Allow
          </button>
        </div>
      </div>
    </div>
  );
}
