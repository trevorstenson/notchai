use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

use crate::adapter::AgentAdapter;
use crate::models::{
    AgentSession, AgentStatus, AgentType, SessionIndexEntry, ToolCallInfo,
    TranscriptContentBlock, TranscriptEntry,
};
use crate::process::{self, ProcessSnapshot};
use crate::scanner::SessionIndexScanner;
use crate::transcript::TranscriptReader;
use crate::util::detect_git_branch;

pub struct ClaudeAdapter {
    scanner: SessionIndexScanner,
    transcript_reader: Mutex<TranscriptReader>,
}

impl ClaudeAdapter {
    pub fn new() -> Self {
        Self {
            scanner: SessionIndexScanner::new(),
            transcript_reader: Mutex::new(TranscriptReader::new()),
        }
    }

    fn resolve_status(
        has_claude_running: bool,
        is_file_active: bool,
        jsonl_age: Option<u64>,
        last_msg_type: Option<&str>,
    ) -> AgentStatus {
        const OPERATING_WINDOW_SECS: u64 = 6;
        const IDLE_WINDOW_SECS: u64 = 20;

        if !has_claude_running || !is_file_active {
            return AgentStatus::Completed;
        }

        let age = jsonl_age.unwrap_or(u64::MAX);

        if age < OPERATING_WINDOW_SECS {
            return AgentStatus::Operating;
        }

        match last_msg_type {
            Some("assistant") => AgentStatus::WaitingForInput,
            _ if age < IDLE_WINDOW_SECS => AgentStatus::Idle,
            _ => AgentStatus::Idle,
        }
    }

    fn is_recent(&self, entry: &SessionIndexEntry) -> bool {
        let modified = match entry.modified.as_deref() {
            Some(m) => m,
            None => return false,
        };

        if let Ok(modified_dt) = chrono::DateTime::parse_from_rfc3339(modified) {
            let now = chrono::Utc::now();
            let age = now - modified_dt.with_timezone(&chrono::Utc);
            age < chrono::Duration::hours(24)
        } else {
            false
        }
    }

    fn decode_project_slug_from_full_path(full_path: &str) -> Option<String> {
        let parent = Path::new(full_path).parent()?;
        let slug = parent.file_name()?.to_str()?;
        Self::decode_project_slug(slug)
    }

    fn decode_project_slug(slug: &str) -> Option<String> {
        if !slug.starts_with('-') || slug.len() <= 1 {
            return None;
        }
        let trimmed = &slug[1..];
        let decoded = format!("/{}", trimmed.replace('-', "/"));
        Some(decoded)
    }

    fn find_session_path(&self, session_id: &str) -> Option<String> {
        let entries = self.scanner.scan_all_projects();
        entries
            .into_iter()
            .find(|e| e.session_id == session_id)
            .map(|e| e.full_path)
    }

    fn summarize_input(input: &serde_json::Value) -> String {
        let s = match input {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Object(map) => {
                let parts: Vec<String> = map
                    .iter()
                    .take(3)
                    .map(|(k, v)| {
                        let val = match v {
                            serde_json::Value::String(s) => s.clone(),
                            _ => v.to_string(),
                        };
                        format!("{}: {}", k, val)
                    })
                    .collect();
                parts.join(", ")
            }
            _ => input.to_string(),
        };
        if s.len() > 200 {
            format!("{}...", &s[..197])
        } else {
            s
        }
    }

    fn summarize_result(result: &serde_json::Value) -> String {
        let s = match result {
            serde_json::Value::String(s) => s.clone(),
            _ => result.to_string(),
        };
        if s.len() > 200 {
            format!("{}...", &s[..197])
        } else {
            s
        }
    }

    fn tool_display_name(name: &str) -> String {
        name.to_string()
    }

    fn parse_tool_calls_from_jsonl(file_path: &str) -> Vec<ToolCallInfo> {
        let path = Path::new(file_path);
        if !path.exists() {
            return Vec::new();
        }

        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let file_size = match file.metadata() {
            Ok(m) => m.len(),
            Err(_) => return Vec::new(),
        };

        // Read last 200KB for tool call extraction
        let max_bytes: u64 = 200_000;
        let read_from = if file_size > max_bytes {
            file_size - max_bytes
        } else {
            0
        };

        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(read_from)).is_err() {
            return Vec::new();
        }

        // Skip partial first line if we seeked to middle
        let mut line = String::new();
        if read_from > 0 {
            let _ = reader.read_line(&mut line);
            line.clear();
        }

        // Collect tool_use and tool_result data
        struct ToolUseData {
            id: String,
            name: String,
            input_summary: String,
            timestamp: Option<String>,
        }

        struct ToolResultData {
            #[allow(dead_code)]
            tool_use_id: String,
            is_error: bool,
            result_preview: Option<String>,
            duration_ms: Option<u64>,
        }

        let mut tool_uses: Vec<ToolUseData> = Vec::new();
        let mut tool_results: HashMap<String, ToolResultData> = HashMap::new();

        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                line.clear();
                continue;
            }

            if let Ok(entry) = serde_json::from_str::<TranscriptEntry>(trimmed) {
                let timestamp = entry.timestamp.clone();

                // Check for top-level tool_result entries
                if entry.entry_type.as_deref() == Some("tool_result") {
                    if let Some(tool_use_id) = entry.tool_use_id {
                        let result_preview = entry.result.as_ref().map(Self::summarize_result);
                        tool_results.insert(
                            tool_use_id.clone(),
                            ToolResultData {
                                tool_use_id,
                                is_error: entry.is_error.unwrap_or(false),
                                result_preview,
                                duration_ms: entry.duration_ms,
                            },
                        );
                    }
                }

                // Check for tool_use blocks in assistant message content
                if let Some(msg) = &entry.message {
                    if let Some(content) = &msg.content {
                        for block in content {
                            match block {
                                TranscriptContentBlock::ToolUse { id, name, input } => {
                                    tool_uses.push(ToolUseData {
                                        id: id.clone(),
                                        name: name.clone(),
                                        input_summary: Self::summarize_input(input),
                                        timestamp: timestamp.clone(),
                                    });
                                }
                                TranscriptContentBlock::ToolResult {
                                    tool_use_id,
                                    content,
                                    is_error,
                                } => {
                                    let result_preview =
                                        content.as_ref().map(Self::summarize_result);
                                    tool_results.insert(
                                        tool_use_id.clone(),
                                        ToolResultData {
                                            tool_use_id: tool_use_id.clone(),
                                            is_error: is_error.unwrap_or(false),
                                            result_preview,
                                            duration_ms: None,
                                        },
                                    );
                                }
                                TranscriptContentBlock::Other => {}
                            }
                        }
                    }
                }
            }

            line.clear();
        }

        // Correlate tool_use with tool_result and build ToolCallInfo list
        let mut calls: Vec<ToolCallInfo> = tool_uses
            .into_iter()
            .map(|tu| {
                let result = tool_results.remove(&tu.id);
                let (status, duration_ms, result_preview) = match result {
                    Some(tr) => {
                        let status = if tr.is_error {
                            "error".to_string()
                        } else {
                            "success".to_string()
                        };
                        (status, tr.duration_ms, tr.result_preview)
                    }
                    None => ("running".to_string(), None, None),
                };

                ToolCallInfo {
                    id: tu.id,
                    tool_name: tu.name.clone(),
                    display_name: Self::tool_display_name(&tu.name),
                    input_summary: tu.input_summary,
                    status,
                    timestamp: tu.timestamp,
                    duration_ms,
                    result_preview,
                }
            })
            .collect();

        // Return last 20
        let len = calls.len();
        if len > 20 {
            calls.drain(..len - 20);
        }

        calls
    }
}

impl AgentAdapter for ClaudeAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Claude
    }

    fn name(&self) -> &str {
        "Claude CLI"
    }

    fn get_tool_calls(&self, session_id: &str) -> Vec<ToolCallInfo> {
        match self.find_session_path(session_id) {
            Some(path) => Self::parse_tool_calls_from_jsonl(&path),
            None => Vec::new(),
        }
    }

    fn get_sessions(&self, snapshot: &ProcessSnapshot) -> Vec<AgentSession> {
        let entries = self.scanner.scan_all_projects();
        let has_claude_running = !snapshot.get_matching_pids(|line| {
            (line.contains("/claude") || line.contains("claude ")) && !line.contains("grep")
        }).is_empty();
        let total_entries = entries.len();
        let recent_entries = entries.iter().filter(|e| self.is_recent(e)).count();
        let active_file_entries = entries
            .iter()
            .filter(|e| process::is_session_active(&e.full_path))
            .count();
        let fallback_active_session = if has_claude_running && active_file_entries == 0 {
            entries
                .iter()
                .filter_map(|e| {
                    process::get_jsonl_age_secs(&e.full_path)
                        .map(|age| (age, e.session_id.clone()))
                })
                .min_by_key(|(age, _)| *age)
                .and_then(|(age, id)| if age < 12 * 60 * 60 { Some(id) } else { None })
        } else {
            None
        };

        let mut reader = self.transcript_reader.lock().unwrap();

        let mut sessions: Vec<AgentSession> = entries
            .iter()
            .map(|entry| {
                reader.read_recent_entries(
                    &entry.session_id,
                    &entry.full_path,
                    50_000,
                );
                let telemetry = reader.get_telemetry(&entry.session_id);

                let jsonl_age = process::get_jsonl_age_secs(&entry.full_path);
                let is_file_active = process::is_session_active(&entry.full_path);
                let is_fallback_active = fallback_active_session
                    .as_ref()
                    .map_or(false, |id| id == &entry.session_id);
                let is_effectively_active = is_file_active || is_fallback_active;

                let status = Self::resolve_status(
                    has_claude_running,
                    is_effectively_active,
                    jsonl_age,
                    telemetry.last_message_type.as_deref(),
                );

                let indexed_project_path = entry.project_path.clone().unwrap_or_default();
                let telemetry_cwd = telemetry.cwd.clone().unwrap_or_default();
                let slug_decoded_path = Self::decode_project_slug_from_full_path(&entry.full_path)
                    .unwrap_or_default();
                let jsonl_parent_path = Path::new(&entry.full_path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                let session_folder_path = if !indexed_project_path.is_empty() {
                    indexed_project_path.clone()
                } else if !telemetry_cwd.is_empty() {
                    telemetry_cwd.clone()
                } else if !slug_decoded_path.is_empty() {
                    slug_decoded_path
                } else {
                    jsonl_parent_path
                };

                let session_folder_name = if !session_folder_path.is_empty() {
                    Path::new(&session_folder_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string()
                } else {
                    "unknown".to_string()
                };

                let project_name = if !indexed_project_path.is_empty() {
                    indexed_project_path
                        .rsplit('/')
                        .next()
                        .unwrap_or("unknown")
                        .to_string()
                } else {
                    session_folder_name.clone()
                };

                let git_branch = entry.git_branch.clone()
                    .filter(|b| !b.is_empty())
                    .or_else(|| detect_git_branch(&session_folder_path))
                    .unwrap_or_default();

                AgentSession {
                    agent_type: AgentType::Claude,
                    id: entry.session_id.clone(),
                    project_path: session_folder_path.clone(),
                    project_name,
                    session_folder_path,
                    session_folder_name,
                    git_branch,
                    first_prompt: entry
                        .first_prompt
                        .as_deref()
                        .unwrap_or("")
                        .chars()
                        .take(200)
                        .collect(),
                    summary: entry.summary.clone(),
                    created: entry.created.clone().unwrap_or_default(),
                    modified: entry.modified.clone().unwrap_or_default(),
                    status,
                    message_count: entry.message_count.unwrap_or(0),
                    total_input_tokens: telemetry.total_input,
                    total_output_tokens: telemetry.total_output,
                    current_task: None,
                    model: telemetry.model,
                    is_sidechain: entry.is_sidechain.unwrap_or(false),
                }
            })
            .collect();

        eprintln!(
            "[notchai] claude adapter: total_entries={} recent_entries={} active_file_entries={} fallback_active={} kept_sessions={} claude_running={}",
            total_entries,
            recent_entries,
            active_file_entries,
            fallback_active_session.is_some(),
            sessions.len(),
            has_claude_running
        );

        sessions.sort_by(|a, b| {
            let a_active = a.status != AgentStatus::Completed;
            let b_active = b.status != AgentStatus::Completed;
            b_active
                .cmp(&a_active)
                .then(b.modified.cmp(&a.modified))
        });

        sessions
    }
}
