use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::adapter::AgentAdapter;
use crate::models::{AgentSession, AgentStatus, AgentType};
use crate::process::ProcessDetector;
use crate::util::detect_git_branch;

// --- Serde structs for Codex JSONL entries ---

#[derive(Debug, Deserialize)]
struct CodexEntry {
    #[serde(rename = "type")]
    entry_type: String,
    payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct SessionMetaPayload {
    id: String,
    timestamp: Option<String>,
    cwd: Option<String>,
    git: Option<CodexGitInfo>,
}

#[derive(Debug, Deserialize)]
struct CodexGitInfo {
    branch: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TurnContextPayload {
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EventMsgPayload {
    #[serde(rename = "type")]
    event_type: Option<String>,
    message: Option<String>,
    info: Option<TokenCountInfo>,
}

#[derive(Debug, Deserialize)]
struct TokenCountInfo {
    total_token_usage: Option<TotalTokenUsage>,
}

#[derive(Debug, Deserialize)]
struct TotalTokenUsage {
    input_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    reasoning_output_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct HistoryEntry {
    session_id: Option<String>,
    text: Option<String>,
}

// --- Parsed session data ---

struct CodexSessionData {
    id: String,
    cwd: String,
    git_branch: Option<String>,
    model: Option<String>,
    created: String,
    first_user_message: Option<String>,
    last_event_type: Option<String>,
    message_count: u32,
    total_input_tokens: u64,
    total_output_tokens: u64,
}

// --- Adapter ---

pub struct CodexAdapter {
    sessions_dir: PathBuf,
    history_path: PathBuf,
    process_detector: ProcessDetector,
}

impl CodexAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            sessions_dir: home.join(".codex").join("sessions"),
            history_path: home.join(".codex").join("history.jsonl"),
            process_detector: ProcessDetector::new(),
        }
    }

    fn is_codex_running(&self) -> bool {
        let output = match Command::new("ps").args(["-eo", "pid,args"]).output() {
            Ok(o) => o,
            Err(_) => return false,
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().any(|line| {
            let line = line.trim();
            (line.contains("/codex") || line.ends_with(" codex") || line.ends_with("/codex"))
                && !line.contains("grep")
                && !line.contains("notchai")
        })
    }

    fn scan_session_files(&self) -> Vec<PathBuf> {
        if !self.sessions_dir.exists() {
            return Vec::new();
        }

        let cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(7 * 24 * 60 * 60))
            .unwrap_or(UNIX_EPOCH);

        let mut files = Vec::new();

        let years = match fs::read_dir(&self.sessions_dir) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };

        for year_entry in years.flatten() {
            let year_path = year_entry.path();
            if !year_path.is_dir() {
                continue;
            }
            let months = match fs::read_dir(&year_path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            for month_entry in months.flatten() {
                let month_path = month_entry.path();
                if !month_path.is_dir() {
                    continue;
                }
                let days = match fs::read_dir(&month_path) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                for day_entry in days.flatten() {
                    let day_path = day_entry.path();
                    if !day_path.is_dir() {
                        continue;
                    }
                    let rollouts = match fs::read_dir(&day_path) {
                        Ok(d) => d,
                        Err(_) => continue,
                    };
                    for file_entry in rollouts.flatten() {
                        let file_path = file_entry.path();
                        let name = file_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("");
                        if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
                            continue;
                        }
                        if let Ok(meta) = fs::metadata(&file_path) {
                            if let Ok(modified) = meta.modified() {
                                if modified >= cutoff {
                                    files.push(file_path);
                                }
                            }
                        }
                    }
                }
            }
        }

        files.sort_by(|a, b| {
            let a_mod = fs::metadata(a)
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH);
            let b_mod = fs::metadata(b)
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH);
            b_mod.cmp(&a_mod)
        });

        files
    }

    fn load_history(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let file = match fs::File::open(&self.history_path) {
            Ok(f) => f,
            Err(_) => return map,
        };
        let reader = BufReader::new(file);
        for line in reader.lines().flatten() {
            if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
                if let (Some(id), Some(text)) = (entry.session_id, entry.text) {
                    map.entry(id).or_insert(text);
                }
            }
        }
        map
    }

    fn parse_session(&self, path: &Path) -> Option<CodexSessionData> {
        let file = fs::File::open(path).ok()?;
        let reader = BufReader::new(file);

        let mut id = None;
        let mut cwd = String::new();
        let mut git_branch = None;
        let mut model = None;
        let mut created = String::new();
        let mut first_user_message = None;
        let mut last_event_type = None;
        let mut message_count: u32 = 0;
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;

        for line in reader.lines().flatten() {
            let entry: CodexEntry = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            match entry.entry_type.as_str() {
                "session_meta" => {
                    if let Ok(meta) = serde_json::from_value::<SessionMetaPayload>(entry.payload) {
                        id = Some(meta.id);
                        cwd = meta.cwd.unwrap_or_default();
                        created = meta.timestamp.unwrap_or_default();
                        if let Some(git) = meta.git {
                            git_branch = git.branch;
                        }
                    }
                }
                "turn_context" => {
                    if let Ok(ctx) = serde_json::from_value::<TurnContextPayload>(entry.payload) {
                        if ctx.model.is_some() {
                            model = ctx.model;
                        }
                    }
                }
                "event_msg" => {
                    if let Ok(evt) = serde_json::from_value::<EventMsgPayload>(entry.payload) {
                        match evt.event_type.as_deref() {
                            Some("user_message") => {
                                message_count += 1;
                                if first_user_message.is_none() {
                                    first_user_message = evt.message;
                                }
                                last_event_type = Some("user_message".to_string());
                            }
                            Some("agent_message") => {
                                message_count += 1;
                                last_event_type = Some("agent_message".to_string());
                            }
                            Some("task_complete") => {
                                last_event_type = Some("task_complete".to_string());
                            }
                            Some("task_started") => {
                                last_event_type = Some("task_started".to_string());
                            }
                            Some("token_count") => {
                                if let Some(info) = evt.info {
                                    if let Some(usage) = info.total_token_usage {
                                        total_input_tokens = usage
                                            .input_tokens
                                            .unwrap_or(0)
                                            + usage.cached_input_tokens.unwrap_or(0);
                                        total_output_tokens = usage
                                            .output_tokens
                                            .unwrap_or(0)
                                            + usage.reasoning_output_tokens.unwrap_or(0);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        let id = id?;

        Some(CodexSessionData {
            id,
            cwd,
            git_branch,
            model,
            created,
            first_user_message,
            last_event_type,
            message_count,
            total_input_tokens,
            total_output_tokens,
        })
    }

    fn resolve_status(
        is_codex_running: bool,
        file_age_secs: Option<u64>,
        last_event_type: Option<&str>,
    ) -> AgentStatus {
        const OPERATING_WINDOW_SECS: u64 = 10;
        const ACTIVE_WINDOW_SECS: u64 = 900;

        let age = file_age_secs.unwrap_or(u64::MAX);

        if !is_codex_running || age > ACTIVE_WINDOW_SECS {
            return AgentStatus::Completed;
        }

        if age < OPERATING_WINDOW_SECS {
            return AgentStatus::Operating;
        }

        match last_event_type {
            Some("task_complete") => AgentStatus::WaitingForInput,
            _ => AgentStatus::Idle,
        }
    }

    fn file_modified_rfc3339(path: &Path) -> String {
        fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| {
                let duration = t.duration_since(UNIX_EPOCH).ok()?;
                let dt = chrono::DateTime::from_timestamp(
                    duration.as_secs() as i64,
                    duration.subsec_nanos(),
                )?;
                Some(dt.to_rfc3339())
            })
            .unwrap_or_default()
    }
}

impl AgentAdapter for CodexAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Codex
    }

    fn name(&self) -> &str {
        "Codex CLI"
    }

    fn get_sessions(&self) -> Vec<AgentSession> {
        let files = self.scan_session_files();
        let is_codex_running = self.is_codex_running();
        let history = self.load_history();
        let total_files = files.len();

        let mut sessions: Vec<AgentSession> = files
            .iter()
            .filter_map(|path| {
                let data = self.parse_session(path)?;
                let file_age = self.process_detector.get_jsonl_age_secs(&path.to_string_lossy());
                let status =
                    Self::resolve_status(is_codex_running, file_age, data.last_event_type.as_deref());

                let first_prompt = history
                    .get(&data.id)
                    .cloned()
                    .or(data.first_user_message)
                    .unwrap_or_default()
                    .chars()
                    .take(200)
                    .collect();

                let project_name = Path::new(&data.cwd)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let session_folder_name = project_name.clone();
                let session_folder_path = data.cwd.clone();

                let git_branch = data
                    .git_branch
                    .filter(|b| !b.is_empty())
                    .or_else(|| detect_git_branch(&data.cwd))
                    .unwrap_or_default();

                let modified = Self::file_modified_rfc3339(path);

                Some(AgentSession {
                    agent_type: AgentType::Codex,
                    id: data.id,
                    project_path: data.cwd.clone(),
                    project_name,
                    session_folder_path,
                    session_folder_name,
                    git_branch,
                    first_prompt,
                    summary: None,
                    created: data.created,
                    modified,
                    status,
                    message_count: data.message_count,
                    total_input_tokens: data.total_input_tokens,
                    total_output_tokens: data.total_output_tokens,
                    current_task: None,
                    model: data.model,
                    is_sidechain: false,
                })
            })
            .collect();

        eprintln!(
            "[notchai] codex adapter: total_files={} codex_running={} sessions={}",
            total_files, is_codex_running, sessions.len()
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
