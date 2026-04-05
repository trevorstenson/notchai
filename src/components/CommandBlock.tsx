import { useState, useMemo } from "react";
import {
  pipelineStageSlices,
  type ShellApprovalModel,
  type RiskSignal,
  type CommandExtraction,
  type LayoutKind,
} from "../lib/shellParse";

interface CommandBlockProps {
  model: ShellApprovalModel;
  layout: LayoutKind;
  defaultExpanded: boolean;
}

export function CommandBlock({ model, layout, defaultExpanded }: CommandBlockProps) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const [showRaw, setShowRaw] = useState(false);

  if (layout === "launcher-script" && model.extraction && !showRaw) {
    return (
      <LauncherScriptBlock
        extraction={model.extraction}
        signals={model.risk.signals}
        expanded={expanded}
        onToggleExpand={() => setExpanded((e) => !e)}
        onShowRaw={() => setShowRaw(true)}
      />
    );
  }

  if (layout === "multiline" && model.extraction?.kind === "long-pipe" && !showRaw) {
    return (
      <PipelineBlock
        command={model.command}
        signals={model.risk.signals}
        expanded={expanded}
        onToggleExpand={() => setExpanded((e) => !e)}
      />
    );
  }

  const isLong = model.command.includes("\n") || model.command.length > 120;
  const lineClass = isLong ? "cmd-block--multiline" : "cmd-block--one-line";

  return (
    <div className="cmd-block-wrap">
      <pre
        className={`cmd-block ${lineClass} ${isLong && !expanded ? "cmd-block--capped" : ""}`}
      >
        <code>
          <span className="cmd-prompt">$ </span>
          <HighlightedCommand text={model.command} signals={model.risk.signals} />
        </code>
      </pre>
      {isLong && !expanded && <div className="cmd-block-fade" aria-hidden />}
      {isLong && (
        <button
          type="button"
          className="cmd-expand-toggle"
          onClick={() => setExpanded((e) => !e)}
          aria-expanded={expanded}
        >
          {expanded ? "Collapse" : "Show full command"}
        </button>
      )}
      {showRaw && layout === "launcher-script" && (
        <button type="button" className="cmd-expand-toggle" onClick={() => setShowRaw(false)}>
          Back to structured view
        </button>
      )}
    </div>
  );
}

function LauncherScriptBlock({
  extraction,
  signals,
  expanded,
  onToggleExpand,
  onShowRaw,
}: {
  extraction: CommandExtraction;
  signals: RiskSignal[];
  expanded: boolean;
  onToggleExpand: () => void;
  onShowRaw: () => void;
}) {
  const isHeredoc = extraction.kind === "heredoc";
  const header = isHeredoc
    ? `${extraction.heredocTarget} << ${extraction.heredocDelimiter}`
    : extraction.launcher ?? "";
  const body = isHeredoc ? extraction.heredocBody : extraction.scriptBody;
  const bodyIsLong = (body?.split("\n").length ?? 0) > 6 || (body?.length ?? 0) > 400;

  return (
    <div className="cmd-block-wrap cmd-block-wrap--split">
      <div className="cmd-block cmd-block--split">
        <div className="cmd-launcher-line">
          <span className="cmd-prompt">$ </span>
          <HighlightedCommand text={header} signals={signals} />
        </div>
        <div className="cmd-split-divider" />
        <div className="cmd-body-wrap">
          <pre
            className={`cmd-body ${bodyIsLong && !expanded ? "cmd-body--capped" : ""}`}
          >
            <code>{body}</code>
          </pre>
          {bodyIsLong && !expanded && <div className="cmd-block-fade" aria-hidden />}
        </div>
      </div>
      <div className="cmd-block-actions">
        {bodyIsLong && (
          <button
            type="button"
            className="cmd-expand-toggle"
            onClick={onToggleExpand}
            aria-expanded={expanded}
          >
            {expanded ? "Collapse" : "Show full script"}
          </button>
        )}
        <button type="button" className="cmd-expand-toggle" onClick={onShowRaw}>
          Show raw command
        </button>
      </div>
    </div>
  );
}

function PipelineBlock({
  command,
  signals,
  expanded,
  onToggleExpand,
}: {
  command: string;
  signals: RiskSignal[];
  expanded: boolean;
  onToggleExpand: () => void;
}) {
  const slices = useMemo(() => pipelineStageSlices(command), [command]);

  return (
    <div className="cmd-block-wrap">
      <pre
        className={`cmd-block cmd-block--multiline ${!expanded ? "cmd-block--capped" : ""}`}
      >
        <code>
          {slices.map((slice, i) => (
            <div key={i} className="cmd-pipeline-stage">
              <span className="cmd-prompt">{i === 0 ? "$ " : "| "}</span>
              <HighlightedCommand
                text={slice.text}
                signals={signalsClippedForSlice(command, signals, slice.globalStart, slice.globalEnd)}
              />
            </div>
          ))}
        </code>
      </pre>
      {!expanded && <div className="cmd-block-fade" aria-hidden />}
      <button
        type="button"
        className="cmd-expand-toggle"
        onClick={onToggleExpand}
        aria-expanded={expanded}
      >
        {expanded ? "Collapse" : "Show full command"}
      </button>
    </div>
  );
}

/** Maps global classifier offsets into a pipeline stage substring (tokens may straddle `|`). */
function signalsClippedForSlice(
  command: string,
  signals: RiskSignal[],
  globalStart: number,
  globalEnd: number,
): RiskSignal[] {
  const out: RiskSignal[] = [];
  for (const s of signals) {
    const tokEnd = s.offset + s.token.length;
    if (tokEnd <= globalStart || s.offset >= globalEnd) continue;
    const clipStart = Math.max(s.offset, globalStart);
    const clipEnd = Math.min(tokEnd, globalEnd);
    if (clipStart >= clipEnd) continue;
    out.push({
      reason: s.reason,
      offset: clipStart - globalStart,
      token: command.slice(clipStart, clipEnd),
    });
  }
  return out;
}

function HighlightedCommand({ text, signals }: { text: string; signals: RiskSignal[] }) {
  const segments = useMemo(() => buildSegments(text, signals), [text, signals]);
  return (
    <>
      {segments.map((seg, i) =>
        seg.highlight ? (
          <span key={i} className="cmd-risk-token">
            {seg.text}
          </span>
        ) : (
          <span key={i}>{seg.text}</span>
        ),
      )}
    </>
  );
}

interface Segment {
  text: string;
  highlight: boolean;
}

function buildSegments(text: string, signals: RiskSignal[]): Segment[] {
  if (signals.length === 0) return [{ text, highlight: false }];

  const relevant = signals
    .filter((s) => s.offset >= 0 && s.offset < text.length)
    .sort((a, b) => a.offset - b.offset || b.token.length - a.token.length);

  if (relevant.length === 0) return [{ text, highlight: false }];

  const segments: Segment[] = [];
  let cursor = 0;

  for (const sig of relevant) {
    if (sig.offset + sig.token.length <= cursor) continue;
    if (sig.offset > cursor) {
      segments.push({ text: text.slice(cursor, sig.offset), highlight: false });
    }
    const start = Math.max(sig.offset, cursor);
    const end = Math.min(sig.offset + sig.token.length, text.length);
    if (end > start) {
      segments.push({ text: text.slice(start, end), highlight: true });
      cursor = end;
    }
  }

  if (cursor < text.length) {
    segments.push({ text: text.slice(cursor), highlight: false });
  }

  return segments;
}
