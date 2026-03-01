import { useState, useMemo } from "react";
import type { PermissionRequestEvent } from "../types/hooks";
import { parseAskUserQuestion } from "../types/hooks";

interface QuestionCardProps {
  approval: PermissionRequestEvent;
  remaining: number;
  respondToApproval: (
    requestId: string,
    decision: string,
    reason?: string,
    updatedInput?: string,
  ) => Promise<void>;
}

export function QuestionCard({
  approval,
  remaining,
  respondToApproval,
}: QuestionCardProps) {
  const questionData = useMemo(
    () => parseAskUserQuestion(approval.toolInput),
    [approval.toolInput],
  );

  const [selections, setSelections] = useState<Record<string, Set<string>>>(
    {},
  );

  if (!questionData || questionData.questions.length === 0) {
    return null;
  }

  const currentQuestion = questionData.questions[0];
  const currentSelections =
    selections[currentQuestion.question] ?? new Set<string>();

  const handleOptionToggle = (label: string) => {
    setSelections((prev) => {
      const prevSet = prev[currentQuestion.question] ?? new Set<string>();
      const newSet = new Set(prevSet);
      if (currentQuestion.multiSelect) {
        if (newSet.has(label)) {
          newSet.delete(label);
        } else {
          newSet.add(label);
        }
      } else {
        newSet.clear();
        newSet.add(label);
      }
      return { ...prev, [currentQuestion.question]: newSet };
    });
  };

  const handleSubmit = () => {
    // Build answers map: question text -> selected label(s)
    const answers: Record<string, string> = {};
    for (const q of questionData.questions) {
      const sel = selections[q.question];
      if (sel && sel.size > 0) {
        answers[q.question] = Array.from(sel).join(", ");
      }
    }

    // Build updatedInput: original tool_input with answers merged in
    let originalInput: Record<string, unknown> = {};
    if (approval.toolInput) {
      try {
        originalInput = JSON.parse(approval.toolInput);
      } catch {
        // fall back to empty
      }
    }
    const updatedInput = JSON.stringify({ ...originalInput, answers });

    respondToApproval(approval.requestId, "allow", undefined, updatedInput);
  };

  const handleDeny = () => {
    respondToApproval(
      approval.requestId,
      "deny",
      "Denied by user in Notchai",
    );
  };

  const hasSelection = currentSelections.size > 0;

  const projectContext = approval.cwd
    ? approval.cwd.split("/").pop() || approval.cwd
    : null;

  return (
    <div className="question-card">
      <div className="question-card-inner">
        <div className="question-card-header">
          <span className="question-card-icon">?</span>
          <span className="question-card-title">
            {currentQuestion.header || "Question"}
          </span>
          {remaining > 0 && (
            <span className="question-card-badge">+{remaining} more</span>
          )}
        </div>

        <div className="question-card-question">
          {currentQuestion.question}
        </div>

        <div className="question-card-options">
          {currentQuestion.options.map((opt) => {
            const isSelected = currentSelections.has(opt.label);
            return (
              <button
                key={opt.label}
                className={`question-card-option ${isSelected ? "question-card-option--selected" : ""}`}
                onClick={() => handleOptionToggle(opt.label)}
              >
                <span className="question-card-option-indicator">
                  {currentQuestion.multiSelect
                    ? isSelected
                      ? "\u2611"
                      : "\u2610"
                    : isSelected
                      ? "\u25CF"
                      : "\u25CB"}
                </span>
                <span className="question-card-option-content">
                  <span className="question-card-option-label">
                    {opt.label}
                  </span>
                  {opt.description && (
                    <span className="question-card-option-desc">
                      {opt.description}
                    </span>
                  )}
                </span>
              </button>
            );
          })}
        </div>

        {projectContext && (
          <div className="question-card-context">{projectContext}</div>
        )}

        <div className="question-card-actions">
          <button
            className="question-card-btn question-card-btn--deny"
            onClick={handleDeny}
          >
            Deny
          </button>
          <button
            className="question-card-btn question-card-btn--submit"
            onClick={handleSubmit}
            disabled={!hasSelection}
          >
            Submit
          </button>
        </div>
      </div>
    </div>
  );
}
