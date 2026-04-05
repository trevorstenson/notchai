import { describe, it, expect } from "vitest";
import {
  buildShellApprovalModel,
  classifyCommand,
  extractStructure,
  pickLayout,
  shouldExpandByDefault,
  riskBadgeText,
} from "./shellParse";

// ── Helpers ──────────────────────────────────────────────────────────────

function model(command: string, description?: string) {
  return buildShellApprovalModel(
    JSON.stringify({ command, description: description ?? null }),
  );
}

// ── 7.1  Simple bash one-liner ───────────────────────────────────────────

describe("7.1 simple one-liner: ls -la src/components/", () => {
  const m = model("ls -la src/components/", "List component files");

  it("parses command and intent", () => {
    expect(m.command).toBe("ls -la src/components/");
    expect(m.intent).toBe("List component files");
  });

  it("classifies as safe", () => {
    expect(m.risk.tier).toBe("safe");
    expect(m.risk.signals).toHaveLength(0);
  });

  it("extracts path", () => {
    expect(m.risk.touchedPaths).toContain("src/components/");
  });

  it("has no extraction", () => {
    expect(m.extraction).toBeNull();
  });

  it("picks one-line layout", () => {
    expect(pickLayout(m)).toBe("one-line");
  });

  it("does not expand by default", () => {
    expect(shouldExpandByDefault(m.risk.tier)).toBe(false);
  });
});

// ── 7.2  Long piped command ──────────────────────────────────────────────

describe("7.2 long-pipe: find | xargs | sort | head", () => {
  const cmd = "find src -name '*.ts' -type f | xargs grep -l 'useState' | sort | head -20";
  const m = model(cmd, "Find components using useState");

  it("classifies as safe", () => {
    expect(m.risk.tier).toBe("safe");
  });

  it("extracts long-pipe with 4 stages", () => {
    expect(m.extraction).not.toBeNull();
    expect(m.extraction!.kind).toBe("long-pipe");
    expect(m.extraction!.pipelineStages).toHaveLength(4);
    expect(m.extraction!.pipelineStages![0]).toContain("find src");
    expect(m.extraction!.pipelineStages![3]).toContain("head -20");
  });

  it("picks multiline layout", () => {
    expect(pickLayout(m)).toBe("multiline");
  });
});

// ── 7.3  python -c ───────────────────────────────────────────────────────

describe("7.3 python3 -c inline script", () => {
  const cmd = `python3 -c 'import json; data = json.load(open("config.json")); print(data.get("version", "unknown"))'`;
  const m = model(cmd, "Read version from config");

  it("classifies as dangerous (inline exec)", () => {
    expect(m.risk.tier).toBe("dangerous");
    expect(m.risk.signals.some(s => s.reason === "runs code")).toBe(true);
  });

  it("extracts inline-script with python language", () => {
    expect(m.extraction).not.toBeNull();
    expect(m.extraction!.kind).toBe("inline-script");
    expect(m.extraction!.launcher).toBe("python3 -c");
    expect(m.extraction!.language).toBe("python");
    expect(m.extraction!.scriptBody).toContain("import json");
    expect(m.extraction!.scriptBody).not.toMatch(/^'/);
  });

  it("extracts config.json path", () => {
    expect(m.risk.touchedPaths).toContain("config.json");
  });

  it("picks launcher-script layout", () => {
    expect(pickLayout(m)).toBe("launcher-script");
  });

  it("expands by default", () => {
    expect(shouldExpandByDefault(m.risk.tier)).toBe(true);
  });

  it("has risk badge", () => {
    expect(riskBadgeText(m.risk)).toContain("runs code");
  });
});

// ── 7.4  node -e ─────────────────────────────────────────────────────────

describe("7.4 node -e inline script", () => {
  const cmd = `node -e "const fs = require('fs'); const pkg = JSON.parse(fs.readFileSync('package.json')); console.log(pkg.version)"`;
  const m = model(cmd, "Print package version");

  it("classifies as dangerous", () => {
    expect(m.risk.tier).toBe("dangerous");
  });

  it("extracts inline-script with javascript language", () => {
    expect(m.extraction).not.toBeNull();
    expect(m.extraction!.kind).toBe("inline-script");
    expect(m.extraction!.launcher).toBe("node -e");
    expect(m.extraction!.language).toBe("javascript");
    expect(m.extraction!.scriptBody).toContain("require");
  });

  it("extracts package.json path", () => {
    expect(m.risk.touchedPaths).toContain("package.json");
  });
});

// ── 7.5  Heredoc ─────────────────────────────────────────────────────────

describe("7.5 heredoc: cat > nginx.conf << EOF", () => {
  const cmd = "cat > nginx.conf << 'EOF'\nserver {\n  listen 80;\n  server_name localhost;\n  location / {\n    proxy_pass http://localhost:3000;\n  }\n}\nEOF";
  const m = model(cmd, "Create nginx config");

  it("classifies as moderate (writes file)", () => {
    expect(m.risk.tier).toBe("moderate");
  });

  it("extracts heredoc", () => {
    expect(m.extraction).not.toBeNull();
    expect(m.extraction!.kind).toBe("heredoc");
    expect(m.extraction!.heredocDelimiter).toBe("EOF");
    expect(m.extraction!.heredocTarget).toContain("cat > nginx.conf");
    expect(m.extraction!.heredocBody).toContain("listen 80");
  });

  it("extracts nginx.conf path", () => {
    expect(m.risk.touchedPaths).toContain("nginx.conf");
  });

  it("picks launcher-script layout", () => {
    expect(pickLayout(m)).toBe("launcher-script");
  });
});

// ── 7.6  File write (redirection) ────────────────────────────────────────

describe("7.6 file write: echo > src/config.ts", () => {
  const m = model("echo 'export default {}' > src/config.ts", "Create empty config module");

  it("classifies as moderate (file write)", () => {
    expect(m.risk.tier).toBe("moderate");
    expect(m.risk.writesFiles).toBe(true);
  });

  it("extracts src/config.ts path", () => {
    expect(m.risk.touchedPaths).toContain("src/config.ts");
  });

  it("picks one-line layout", () => {
    expect(pickLayout(m)).toBe("one-line");
  });

  it("has amber risk badge", () => {
    expect(riskBadgeText(m.risk)).toContain("writes file");
  });
});

// ── 7.7  Destructive command ─────────────────────────────────────────────

describe("7.7 destructive: rm -rf node_modules/ dist/ .next/", () => {
  const m = model("rm -rf node_modules/ dist/ .next/", "Clean build artifacts");

  it("classifies as dangerous (destructive)", () => {
    expect(m.risk.tier).toBe("dangerous");
    expect(m.risk.isDestructive).toBe(true);
  });

  it("signals contain rm", () => {
    expect(m.risk.signals.some(s => s.token === "rm")).toBe(true);
  });

  it("extracts all three paths", () => {
    expect(m.risk.touchedPaths).toContain("node_modules/");
    expect(m.risk.touchedPaths).toContain("dist/");
    expect(m.risk.touchedPaths).toContain(".next/");
  });

  it("picks one-line layout", () => {
    expect(pickLayout(m)).toBe("one-line");
  });

  it("expands by default", () => {
    expect(shouldExpandByDefault(m.risk.tier)).toBe(true);
  });
});

// ── 7.8  Network fetch ───────────────────────────────────────────────────

describe("7.8 network: curl | bash", () => {
  const cmd = "curl -sSL https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash";
  const m = model(cmd, "Install nvm");

  it("classifies as dangerous (network + pipe-to-exec)", () => {
    expect(m.risk.tier).toBe("dangerous");
    expect(m.risk.requiresNetwork).toBe(true);
  });

  it("has curl and bash signals", () => {
    expect(m.risk.signals.some(s => s.token === "curl")).toBe(true);
    expect(m.risk.signals.some(s => s.token.includes("bash"))).toBe(true);
  });

  it("extracts long-pipe with 2 stages (remote script pipe)", () => {
    expect(m.extraction).not.toBeNull();
    expect(m.extraction!.kind).toBe("long-pipe");
    expect(m.extraction!.pipelineStages).toHaveLength(2);
    expect(m.extraction!.pipelineStages![0]).toMatch(/curl/);
    expect(m.extraction!.pipelineStages![1]).toMatch(/bash/);
  });

  it("expands by default", () => {
    expect(shouldExpandByDefault(m.risk.tier)).toBe(true);
  });

  it("has runs remote code badge", () => {
    expect(riskBadgeText(m.risk, m.command)).toContain("runs remote code");
  });
});

// ── 7.9  Command with many flags ─────────────────────────────────────────

describe("7.9 many flags: long git log", () => {
  const cmd = "git log --oneline --graph --decorate --all --author='Trevor Stenson' --since='2025-01-01' --until='2025-12-31' --stat -- src/";
  const m = model(cmd, "Show commit history for src");

  it("classifies as safe", () => {
    expect(m.risk.tier).toBe("safe");
  });

  it("extracts src/ path", () => {
    expect(m.risk.touchedPaths).toContain("src/");
  });

  it("picks multiline layout (length > 120)", () => {
    expect(cmd.length).toBeGreaterThan(120);
    expect(pickLayout(m)).toBe("multiline");
  });

  it("does not expand by default", () => {
    expect(shouldExpandByDefault(m.risk.tier)).toBe(false);
  });
});

// ── 7.10 sudo command ────────────────────────────────────────────────────

describe("7.10 sudo: chown", () => {
  const m = model(
    "sudo chown -R $(whoami) /usr/local/lib/node_modules",
    "Fix npm permissions",
  );

  it("classifies as dangerous (privilege escalation)", () => {
    expect(m.risk.tier).toBe("dangerous");
    expect(m.risk.requiresPrivilege).toBe(true);
  });

  it("signals contain sudo", () => {
    expect(m.risk.signals.some(s => s.token === "sudo")).toBe(true);
  });

  it("extracts /usr/local/lib/node_modules path", () => {
    expect(m.risk.touchedPaths).toContain("/usr/local/lib/node_modules");
  });

  it("has sudo badge", () => {
    expect(riskBadgeText(m.risk, m.command)).toContain("sudo");
  });
});

// ── Edge cases ───────────────────────────────────────────────────────────

describe("edge cases", () => {
  it("empty command → unknown tier", () => {
    const r = classifyCommand("");
    expect(r.tier).toBe("unknown");
  });

  it("whitespace-only command → unknown tier", () => {
    const r = classifyCommand("   \n  ");
    expect(r.tier).toBe("unknown");
  });

  it("null toolInput → empty model", () => {
    const m = buildShellApprovalModel(null);
    expect(m.command).toBe("");
    expect(m.intent).toBeNull();
    expect(m.risk.tier).toBe("unknown");
  });

  it("non-JSON toolInput → uses raw string as command", () => {
    const m = buildShellApprovalModel("ls -la");
    expect(m.command).toBe("ls -la");
    expect(m.intent).toBeNull();
  });

  it("pipes inside quotes don't split pipeline", () => {
    const ext = extractStructure("echo 'hello | world' | grep hello | sort | uniq");
    expect(ext).not.toBeNull();
    expect(ext!.kind).toBe("long-pipe");
    expect(ext!.pipelineStages).toHaveLength(4);
    expect(ext!.pipelineStages![0]).toContain("'hello | world'");
  });

  it("very long single command without pipes → multiline layout", () => {
    const long = "echo " + "a".repeat(200);
    const m = model(long);
    expect(pickLayout(m)).toBe("multiline");
  });

  it("short two-stage pipe without remote fetch is not long-pipe extraction", () => {
    const m = model("ls | grep foo");
    expect(m.extraction).toBeNull();
    expect(pickLayout(m)).toBe("one-line");
  });

  it("git status is safe", () => {
    expect(classifyCommand("git status").tier).toBe("safe");
  });

  it("git push is moderate", () => {
    expect(classifyCommand("git push origin main").tier).toBe("moderate");
  });

  it("npm install is moderate", () => {
    expect(classifyCommand("npm install express").tier).toBe("moderate");
  });

  it("docker run is safe (no explicit signal)", () => {
    // docker itself isn't in our signal list; it's just a binary
    expect(classifyCommand("docker ps").tier).toBe("safe");
  });
});

describe("inline-script unescaping", () => {
  it("single-quoted body strips outer quotes", () => {
    const ext = extractStructure("python3 -c 'print(42)'");
    expect(ext?.scriptBody).toBe("print(42)");
  });

  it("double-quoted body unescapes backslashes", () => {
    const ext = extractStructure('node -e "console.log(\\"hello\\")"');
    expect(ext?.scriptBody).toBe('console.log("hello")');
  });

  it("$-quoted body processes ANSI escapes", () => {
    const ext = extractStructure("python3 -c $'print(\\'hello\\')\\nprint(\\'world\\')'");
    expect(ext?.scriptBody).toContain("\n");
  });
});
