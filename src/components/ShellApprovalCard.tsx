import { useMemo } from "react";
import type { PermissionRequestEvent } from "../types/hooks";
import {
  buildShellApprovalModel,
  pickLayout,
  shouldExpandByDefault,
  riskBadgeText,
} from "../lib/shellParse";
import { CommandBlock } from "./CommandBlock";

interface ShellApprovalCardProps {
  approval: PermissionRequestEvent;
  remaining: number;
  /** Shown in header — Claude uses "Bash"; keep "Shell" for generic tools */
  displayToolName?: string;
  children: React.ReactNode; // ApprovalActions slot
}

const TIER_COLORS: Record<string, string> = {
  dangerous: "#FF4444",
  moderate: "#FFB800",
  unknown: "#FF4444",
};

export function ShellApprovalCard({
  approval,
  remaining,
  displayToolName = "Bash",
  children,
}: ShellApprovalCardProps) {
  const model = useMemo(
    () => buildShellApprovalModel(approval.toolInput),
    [approval.toolInput],
  );

  const layout = useMemo(() => pickLayout(model), [model]);
  const defaultExpanded = useMemo(() => shouldExpandByDefault(model.risk.tier), [model.risk.tier]);
  const badge = useMemo(
    () => riskBadgeText(model.risk, model.command),
    [model.risk, model.command],
  );
  const tierColor = TIER_COLORS[model.risk.tier] ?? undefined;

  const cardClass = model.risk.tier === "dangerous" || model.risk.tier === "unknown"
    ? "tool-approval-card shell-card--danger"
    : model.risk.tier === "moderate"
      ? "tool-approval-card shell-card--moderate"
      : "tool-approval-card";

  const titleId = `shell-approval-title-${approval.requestId}`;
  const uniqueReasons = useMemo(() => {
    if (model.risk.tier !== "dangerous" || model.risk.signals.length === 0) return [];
    const seen = new Set<string>();
    const out: string[] = [];
    for (const s of model.risk.signals) {
      if (!seen.has(s.reason)) {
        seen.add(s.reason);
        out.push(s.reason);
      }
    }
    return out;
  }, [model.risk.tier, model.risk.signals]);

  return (
    <div className="tool-approval">
      <div
        className={cardClass}
        role="region"
        aria-labelledby={titleId}
      >
        {/* Header */}
        <div className="tool-approval-header">
          <span className="tool-approval-icon">{"\u25B6"}</span>
          <span className="tool-approval-title" id={titleId}>
            {displayToolName}
          </span>
          {badge && (
            <span className="shell-risk-badge" style={tierColor ? { color: tierColor, borderColor: tierColor } : undefined}>
              {badge}
            </span>
          )}
          {remaining > 0 && (
            <span className="tool-approval-badge">+{remaining} more</span>
          )}
        </div>

        {/* Command block */}
        <CommandBlock model={model} layout={layout} defaultExpanded={defaultExpanded} />

        {model.risk.tier === "unknown" && model.command.trim().length > 0 && (
          <p className="shell-unknown-warning" role="status">
            Could not parse this command. Review the raw text carefully before approving.
          </p>
        )}

        {/* Intent */}
        {model.intent && (
          <div className="shell-intent">{model.intent}</div>
        )}

        {uniqueReasons.length > 1 && (
          <ul className="shell-risk-signal-list" aria-label="Risk factors">
            {uniqueReasons.map((reason) => (
              <li key={reason}>{reason}</li>
            ))}
          </ul>
        )}

        {/* Touched paths */}
        {model.risk.touchedPaths.length > 0 && (
          <div className="shell-touched-paths">
            {model.risk.touchedPaths.map((p) => (
              <span key={p} className="shell-path-chip">
                <span className="shell-path-icon">{p.endsWith("/") ? "\uD83D\uDCC1" : "\uD83D\uDCC4"}</span>
                {p}
              </span>
            ))}
          </div>
        )}

        {/* Actions (passed in from ToolApproval) */}
        {children}
      </div>
    </div>
  );
}
