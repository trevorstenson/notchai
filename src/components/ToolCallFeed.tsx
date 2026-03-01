import { useSessionToolCalls } from "../hooks/useSessionToolCalls";
import type { ToolCallInfo } from "../types";

interface ToolCallFeedProps {
  sessionId: string;
}

function StatusIcon({ status }: { status: string }) {
  if (status === "success") {
    return <span className="tool-call-icon tool-call-icon--success">&#10003;</span>;
  }
  if (status === "error") {
    return <span className="tool-call-icon tool-call-icon--error">&#10007;</span>;
  }
  return <span className="tool-call-icon tool-call-icon--running">&#8226;</span>;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

function ToolCallRow({ call }: { call: ToolCallInfo }) {
  return (
    <div className="tool-call-row">
      <StatusIcon status={call.status} />
      <span className="tool-call-name">{call.displayName}</span>
      {call.durationMs != null && (
        <span className="tool-call-duration">{formatDuration(call.durationMs)}</span>
      )}
      {call.inputSummary && (
        <div className="tool-call-input">{call.inputSummary}</div>
      )}
    </div>
  );
}

export function ToolCallFeed({ sessionId }: ToolCallFeedProps) {
  const { toolCalls, loading } = useSessionToolCalls(sessionId);

  if (loading && toolCalls.length === 0) {
    return (
      <div className="tool-call-feed">
        <div className="tool-call-feed-empty">Loading tool calls...</div>
      </div>
    );
  }

  if (toolCalls.length === 0) {
    return (
      <div className="tool-call-feed">
        <div className="tool-call-feed-empty">No tool calls yet</div>
      </div>
    );
  }

  return (
    <div className="tool-call-feed">
      {toolCalls.map((call) => (
        <ToolCallRow key={call.id} call={call} />
      ))}
    </div>
  );
}
