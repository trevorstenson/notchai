// ── Types ────────────────────────────────────────────────────────────────

export interface ShellApprovalModel {
  command: string;
  intent: string | null;
  risk: RiskMetadata;
  extraction: CommandExtraction | null;
}

export interface RiskMetadata {
  tier: "safe" | "moderate" | "dangerous" | "unknown";
  signals: RiskSignal[];
  touchedPaths: string[];
  requiresNetwork: boolean;
  requiresPrivilege: boolean;
  isDestructive: boolean;
  writesFiles: boolean;
}

export interface RiskSignal {
  token: string;
  offset: number;
  reason: string;
}

export interface CommandExtraction {
  kind: "inline-script" | "heredoc" | "long-pipe";
  launcher: string | null;
  scriptBody: string | null;
  language: "python" | "javascript" | "ruby" | "perl" | "bash" | null;
  heredocDelimiter: string | null;
  heredocTarget: string | null;
  heredocBody: string | null;
  pipelineStages: string[] | null;
}

export type LayoutKind = "one-line" | "multiline" | "launcher-script";

// ── Signal definitions ───────────────────────────────────────────────────

interface SignalDef {
  id: string;
  pattern: RegExp;
  reason: string;
  tier: "dangerous" | "moderate";
}

const SIGNAL_DEFS: SignalDef[] = [
  // Dangerous (ordered by severity)
  { id: "privilege",   pattern: /\b(sudo|doas|pkexec)\b/g,                                          reason: "escalates privileges", tier: "dangerous" },
  { id: "destructive", pattern: /\b(rm|rmdir|truncate|shred|dd|mkfs)\b/g,                           reason: "deletes files",        tier: "dangerous" },
  { id: "network",     pattern: /\b(curl|wget|fetch|ssh|scp|rsync|nc|ncat|socat)\b/g,               reason: "network access",       tier: "dangerous" },
  { id: "pipe_exec",   pattern: /\|\s*(ba)?sh\b/g,                                                  reason: "pipes to shell",       tier: "dangerous" },
  { id: "pipe_exec2",  pattern: /\|\s*(python3?|node|ruby|perl)\b/g,                                reason: "pipes to interpreter", tier: "dangerous" },
  { id: "eval",        pattern: /\b(eval|exec)\b/g,                                                 reason: "dynamic execution",    tier: "dangerous" },
  { id: "inline_exec", pattern: /\b(python3?|node|ruby|perl)\s+-[ec]\b/g,                           reason: "runs code",            tier: "dangerous" },
  // Moderate
  { id: "file_write",  pattern: /[12]?>>?(?!=)/g,                                                   reason: "writes file",          tier: "moderate" },
  { id: "file_write2", pattern: /\b(tee|cp|mv|install)\b/g,                                         reason: "writes file",          tier: "moderate" },
  { id: "package_mgr", pattern: /\b(npm|yarn|pnpm|pip|pip3|brew|apt|cargo)\s+(install|add|remove|uninstall)\b/g, reason: "modifies packages", tier: "moderate" },
  { id: "git_mutate",  pattern: /\bgit\s+(push|reset|rebase|merge|cherry-pick|stash|clean)\b/g,     reason: "mutates git",          tier: "moderate" },
];

// ── Risk classification ──────────────────────────────────────────────────

export function classifyCommand(command: string): RiskMetadata {
  if (!command.trim() || looksLikeBinary(command)) {
    return { tier: "unknown", signals: [], touchedPaths: [], requiresNetwork: false, requiresPrivilege: false, isDestructive: false, writesFiles: false };
  }

  const signals: RiskSignal[] = [];
  let highestTier: "safe" | "moderate" | "dangerous" = "safe";

  for (const def of SIGNAL_DEFS) {
    const regex = new RegExp(def.pattern.source, def.pattern.flags);
    let match: RegExpExecArray | null;
    while ((match = regex.exec(command)) !== null) {
      signals.push({ token: match[0], offset: match.index, reason: def.reason });
      if (def.tier === "dangerous") highestTier = "dangerous";
      else if (def.tier === "moderate" && highestTier !== "dangerous") highestTier = "moderate";
    }
  }

  // Also flag URLs as network (excluding localhost/127.0.0.1)
  const urlRe = /https?:\/\/[^\s)'"]+/g;
  let urlMatch: RegExpExecArray | null;
  while ((urlMatch = urlRe.exec(command)) !== null) {
    if (/^https?:\/\/(localhost|127\.0\.0\.1)(:|\/|$)/.test(urlMatch[0])) continue;
    if (!signals.some(s => s.offset === urlMatch!.index)) {
      signals.push({ token: urlMatch[0], offset: urlMatch.index, reason: "network access" });
      if (highestTier !== "dangerous") highestTier = "dangerous";
    }
  }

  return {
    tier: highestTier,
    signals,
    touchedPaths: extractPaths(command),
    requiresNetwork: signals.some(s => s.reason === "network access"),
    requiresPrivilege: signals.some(s => s.reason === "escalates privileges"),
    isDestructive: signals.some(s => s.reason === "deletes files"),
    writesFiles: signals.some(s => s.reason === "writes file"),
  };
}

function looksLikeBinary(s: string): boolean {
  // Control chars (except tab, newline, carriage return) suggest binary content
  const controlCount = [...s].filter(c => {
    const code = c.charCodeAt(0);
    return code < 32 && code !== 9 && code !== 10 && code !== 13;
  }).length;
  return controlCount > s.length * 0.1;
}

// ── Path extraction ──────────────────────────────────────────────────────

function extractPaths(command: string): string[] {
  const paths = new Set<string>();
  const tokens = tokenizeForPaths(command);
  for (const t of tokens) {
    if (looksLikePath(t)) paths.add(t);
  }
  return [...paths];
}

function extractPathsFromText(text: string): string[] {
  const paths = new Set<string>();
  // Look for path-like strings including inside quotes
  const quotedPaths = text.matchAll(/['"]([^'"]+\.[a-zA-Z]{1,10})['"]/g);
  for (const m of quotedPaths) {
    if (looksLikePath(m[1])) paths.add(m[1]);
  }
  // Also scan bare tokens
  for (const t of text.split(/[\s;,()]+/).filter(Boolean)) {
    const cleaned = t.replace(/^['"]|['"]$/g, "");
    if (looksLikePath(cleaned)) paths.add(cleaned);
  }
  return [...paths];
}

function tokenizeForPaths(command: string): string[] {
  const tokens: string[] = [];
  // Remove known command names, flags, and operators; split on whitespace
  const stripped = command
    .replace(/<<-?\s*'?[A-Za-z_]+\b'?/g, " ")  // heredoc markers
    .replace(/[12]?>>?/g, " ")                    // redirections (but keep the target)
    .replace(/\|/g, " ");                          // pipes
  const parts = stripped.split(/\s+/).filter(Boolean);
  for (const p of parts) {
    const cleaned = p.replace(/^['"]|['"]$/g, "").replace(/[;,)]+$/, "");
    if (cleaned) tokens.push(cleaned);
  }
  return tokens;
}

function looksLikePath(token: string): boolean {
  if (token.startsWith("-")) return false;
  if (/^[a-z_][a-z_0-9]*$/i.test(token) && !token.includes(".") && !token.includes("/")) return false;
  if (token.startsWith("./") || token.startsWith("../") || token.startsWith("/") || token.startsWith("~/")) return true;
  if (/\.[a-zA-Z]{1,10}$/.test(token) && !token.startsWith("http")) return true;
  if (token.includes("/") && !token.startsWith("http")) return true;
  return false;
}

// ── Structured extraction ────────────────────────────────────────────────

export function extractStructure(command: string): CommandExtraction | null {
  return extractInlineScript(command)
    ?? extractHeredoc(command)
    ?? extractLongPipe(command);
}

const INLINE_SCRIPT_RE = /^(.*?\b(?:python3?|node|ruby|perl)\s+-[ec])\s+(.+)$/s;
const LANGUAGE_MAP: Record<string, CommandExtraction["language"]> = {
  python: "python", python3: "python",
  node: "javascript",
  ruby: "ruby",
  perl: "perl",
};

function extractInlineScript(command: string): CommandExtraction | null {
  const match = command.match(INLINE_SCRIPT_RE);
  if (!match) return null;

  const launcher = match[1].trim();
  const rawBody = match[2];
  const scriptBody = unescapeScriptBody(rawBody);

  const binaryMatch = launcher.match(/\b(python3?|node|ruby|perl)\b/);
  const language = binaryMatch ? (LANGUAGE_MAP[binaryMatch[1]] ?? null) : null;

  return {
    kind: "inline-script",
    launcher,
    scriptBody,
    language,
    heredocDelimiter: null,
    heredocTarget: null,
    heredocBody: null,
    pipelineStages: null,
  };
}

function unescapeScriptBody(raw: string): string {
  const trimmed = raw.trim();
  if (trimmed.startsWith("$'") && trimmed.endsWith("'")) {
    return trimmed.slice(2, -1)
      .replace(/\\n/g, "\n").replace(/\\t/g, "\t")
      .replace(/\\'/g, "'").replace(/\\\\/g, "\\");
  }
  if (trimmed.startsWith("'") && trimmed.endsWith("'")) {
    return trimmed.slice(1, -1).replace(/'\\'''/g, "'");
  }
  if (trimmed.startsWith('"') && trimmed.endsWith('"')) {
    return trimmed.slice(1, -1)
      .replace(/\\"/g, '"').replace(/\\\\/g, "\\")
      .replace(/\\\$/g, "$").replace(/\\`/g, "`");
  }
  return trimmed;
}

const HEREDOC_RE = /^(.*?)<<-?\s*'?([A-Za-z_][A-Za-z_0-9]*)'?\s*\n([\s\S]*?)\n\2\s*$/;

function extractHeredoc(command: string): CommandExtraction | null {
  const match = command.match(HEREDOC_RE);
  if (!match) return null;

  return {
    kind: "heredoc",
    launcher: null,
    scriptBody: null,
    language: null,
    heredocDelimiter: match[2],
    heredocTarget: match[1].trim(),
    heredocBody: match[3],
    pipelineStages: null,
  };
}

function extractLongPipe(command: string): CommandExtraction | null {
  const slices = pipelineStageSlices(command);
  if (slices.length < 2) return null;
  const stages = slices.map((s) => s.text);
  const qualifies =
    slices.length >= 3 ||
    command.length > 120 ||
    isShortRemoteScriptPipe(stages);
  if (!qualifies) return null;
  return {
    kind: "long-pipe",
    launcher: null,
    scriptBody: null,
    language: null,
    heredocDelimiter: null,
    heredocTarget: null,
    heredocBody: null,
    pipelineStages: stages,
  };
}

/** e.g. `curl https://…/install.sh | bash` — 2 stages, spec §7.8 still uses long-pipe layout */
function isShortRemoteScriptPipe(stages: string[]): boolean {
  if (stages.length !== 2) return false;
  const [a, b] = stages;
  if (!/https?:\/\//i.test(a)) return false;
  if (!/\b(curl|wget|fetch)\b/i.test(a)) return false;
  return /\b(ba)?sh\b|\bpython3?\b|\bnode\b|\bruby\b|\bperl\b/i.test(b);
}

/** Byte ranges per stage; `raw` may include leading/trailing spaces. */
function splitPipelineWithRanges(command: string): { raw: string; start: number; end: number }[] {
  const stages: { raw: string; start: number; end: number }[] = [];
  let current = "";
  let stageStart = 0;
  let depth = 0;
  let inSingle = false;
  let inDouble = false;

  for (let i = 0; i < command.length; i++) {
    const ch = command[i];
    const prev = i > 0 ? command[i - 1] : "";

    if (ch === "'" && !inDouble && prev !== "\\") { inSingle = !inSingle; current += ch; continue; }
    if (ch === '"' && !inSingle && prev !== "\\") { inDouble = !inDouble; current += ch; continue; }
    if (inSingle || inDouble) { current += ch; continue; }

    if (ch === "(" || ch === "{") { depth++; current += ch; continue; }
    if (ch === ")" || ch === "}") { depth--; current += ch; continue; }

    if (ch === "|" && depth === 0 && command[i + 1] !== "|") {
      stages.push({ raw: current, start: stageStart, end: i });
      current = "";
      stageStart = i + 1;
      continue;
    }

    current += ch;
  }
  if (current.trim()) {
    stages.push({ raw: current, start: stageStart, end: command.length });
  }
  return stages;
}

export interface PipelineStageSlice {
  /** Trimmed stage text for display */
  text: string;
  /** Start index in full command for trimmed text (for risk highlights) */
  globalStart: number;
  /** End index (exclusive) in full command */
  globalEnd: number;
}

/** Maps a piped command into display stages with global string indices for token highlighting. */
export function pipelineStageSlices(command: string): PipelineStageSlice[] {
  const ranges = splitPipelineWithRanges(command);
  const out: PipelineStageSlice[] = [];
  for (const r of ranges) {
    const raw = r.raw;
    if (!raw.trim()) continue;
    let li = 0;
    while (li < raw.length && /\s/.test(raw[li])) li++;
    let lj = raw.length - 1;
    while (lj >= li && /\s/.test(raw[lj])) lj--;
    const text = raw.slice(li, lj + 1);
    out.push({
      text,
      globalStart: r.start + li,
      globalEnd: r.start + lj + 1,
    });
  }
  return out;
}

// ── Layout selection ─────────────────────────────────────────────────────

export function pickLayout(model: ShellApprovalModel): LayoutKind {
  if (model.extraction?.kind === "inline-script" || model.extraction?.kind === "heredoc") {
    return "launcher-script";
  }
  if (model.extraction?.kind === "long-pipe") {
    return "multiline";
  }
  if (model.command.includes("\n") || model.command.length > 120) {
    return "multiline";
  }
  return "one-line";
}

export function shouldExpandByDefault(tier: RiskMetadata["tier"]): boolean {
  return tier === "dangerous" || tier === "unknown";
}

// ── Top-level risk badge text ────────────────────────────────────────────

/** Maps internal classifier reasons to short header labels (spec §6 R6). */
const BADGE_LABEL: Record<string, string> = {
  "escalates privileges": "sudo",
  "deletes files": "deletes files",
  "network access": "network access",
  "pipes to shell": "runs code",
  "pipes to interpreter": "runs code",
  "dynamic execution": "runs code",
  "runs code": "runs code",
  "writes file": "writes file",
  "modifies packages": "modifies packages",
  "mutates git": "mutates git",
};

/** Lower = higher priority for picking a single badge */
const BADGE_PRIORITY: Record<string, number> = {
  "escalates privileges": 0,
  "deletes files": 1,
  "network access": 3,
  "pipes to shell": 4,
  "pipes to interpreter": 4,
  "dynamic execution": 4,
  "runs code": 4,
  "writes file": 5,
  "modifies packages": 5,
  "mutates git": 5,
};

function commandHasRemoteUrl(command: string): boolean {
  const urlRe = /https?:\/\/[^\s)'"]+/g;
  let m: RegExpExecArray | null;
  while ((m = urlRe.exec(command)) !== null) {
    if (/^https?:\/\/(localhost|127\.0\.0\.1)(:|\/|$)/.test(m[0])) continue;
    return true;
  }
  return false;
}

/**
 * Header risk badge. Pass `command` so we can detect `curl … | bash` → "runs remote code".
 */
export function riskBadgeText(risk: RiskMetadata, command = ""): string | null {
  if (risk.tier === "safe") return null;
  if (risk.tier === "unknown") return "\u26A0 unrecognized";

  if (risk.signals.length === 0) return null;

  const pipeShell = risk.signals.some(s => s.reason === "pipes to shell");
  if (pipeShell && commandHasRemoteUrl(command)) {
    return "\u26A0 runs remote code";
  }

  let bestReason: string | null = null;
  let bestP = 999;
  for (const s of risk.signals) {
    const p = BADGE_PRIORITY[s.reason] ?? 50;
    if (p < bestP) {
      bestP = p;
      bestReason = s.reason;
    }
  }
  if (!bestReason) return null;
  const label = BADGE_LABEL[bestReason] ?? bestReason;
  return `\u26A0 ${label}`;
}

// ── Entry point ──────────────────────────────────────────────────────────

export function buildShellApprovalModel(
  toolInput: string | null,
): ShellApprovalModel {
  let command = "";
  let intent: string | null = null;

  if (toolInput) {
    try {
      const parsed = JSON.parse(toolInput);
      command = parsed.command ?? toolInput;
      intent = parsed.description ?? null;
    } catch {
      command = toolInput;
    }
  }

  const risk = classifyCommand(command);
  const extraction = extractStructure(command);

  // Merge paths found inside extracted script/heredoc bodies
  if (extraction) {
    const bodyText = extraction.scriptBody ?? extraction.heredocBody ?? "";
    if (bodyText) {
      for (const p of extractPathsFromText(bodyText)) {
        if (!risk.touchedPaths.includes(p)) risk.touchedPaths.push(p);
      }
    }
  }

  return { command, intent, risk, extraction };
}
