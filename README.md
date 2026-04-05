# Notchai

A macOS notch-bar app that monitors your AI coding agent sessions in real time.

Notchai sits in your MacBook's notch area as a compact Dynamic Island. Hover to expand and see all your running sessions — click to jump straight into the terminal.

## Features

- **Real-time session monitoring** — polls running sessions every 3 seconds (hooks and the event bus still trigger immediate refreshes when available)
- **Collapsed + expanded views** — status dots at the notch, full session list on hover
- **Cost tracking** — per-session and daily totals based on token usage
- **Native notifications** — alerts when a session needs input or completes
- **Terminal integration** — click to focus the right terminal window, or resume a finished session
- **Global shortcut** — `Cmd+Shift+N` to toggle from anywhere

## Supported Agents

- Claude CLI
- OpenAI Codex
- Cursor

## Tech Stack

Tauri 2 (Rust) + React 19 + TypeScript + Vite

## Getting Started

**Prerequisites:** macOS (with notch support recommended), Node.js, Rust

```sh
# Install dependencies
npm install

# Run in development
npm run tauri dev

# Build for production
npm run tauri build
```

## Project Structure

```
src/                  # React frontend
  components/         # CollapsedView, ExpandedView, StatusDot
  hooks/              # useAgentMonitor, useSessionNotifications
  lib/                # Pricing calculations

src-tauri/src/        # Rust backend
  adapters/           # Claude, Codex, Cursor session discovery
  monitor.rs          # Session aggregation
  scanner.rs          # Project directory scanning
  transcript.rs       # JSONL transcript parsing
  process.rs          # Process detection
  notch.rs            # macOS notch detection (objc)
  lib.rs              # Tauri commands & terminal routing
```
