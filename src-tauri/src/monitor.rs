use std::process::Command;
use std::sync::Mutex;
use std::path::Path;

use crate::models::{AgentSession, AgentStatus, SessionIndexEntry};
use crate::process::ProcessDetector;
use crate::scanner::SessionIndexScanner;
use crate::transcript::TranscriptReader;

pub struct AgentMonitor {
    scanner: SessionIndexScanner,
    transcript_reader: Mutex<TranscriptReader>,
    process_detector: ProcessDetector,
}

impl AgentMonitor {
    pub fn new() -> Self {
        Self {
            scanner: SessionIndexScanner::new(),
            transcript_reader: Mutex::new(TranscriptReader::new()),
            process_detector: ProcessDetector::new(),
        }
    }

    pub fn get_sessions(&self) -> Vec<AgentSession> {
        let entries = self.scanner.scan_all_projects();
        let has_claude_running = self.process_detector.is_any_claude_running();
        let total_entries = entries.len();
        let recent_entries = entries.iter().filter(|e| self.is_recent(e)).count();
        let active_file_entries = entries
            .iter()
            .filter(|e| self.process_detector.is_session_active(&e.full_path))
            .count();
        let fallback_active_session = if has_claude_running && active_file_entries == 0 {
            entries
                .iter()
                .filter_map(|e| {
                    self.process_detector
                        .get_jsonl_age_secs(&e.full_path)
                        .map(|age| (age, e.session_id.clone()))
                })
                .min_by_key(|(age, _)| *age)
                // If Claude is running and we found a reasonably fresh transcript,
                // treat the freshest one as active as a fallback heuristic.
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

                let jsonl_age = self.process_detector.get_jsonl_age_secs(&entry.full_path);
                let is_file_active = self.process_detector.is_session_active(&entry.full_path);
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
            "[notchai] get_sessions total_entries={} recent_entries={} active_file_entries={} fallback_active={} kept_sessions={} claude_running={}",
            total_entries,
            recent_entries,
            active_file_entries,
            fallback_active_session.is_some(),
            sessions.len(),
            has_claude_running
        );

        // Active sessions first, then by modified date
        sessions.sort_by(|a, b| {
            let a_active = a.status != AgentStatus::Completed;
            let b_active = b.status != AgentStatus::Completed;
            b_active
                .cmp(&a_active)
                .then(b.modified.cmp(&a.modified))
        });

        sessions
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
}

fn detect_git_branch(project_dir: &str) -> Option<String> {
    if project_dir.is_empty() {
        return None;
    }
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(project_dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() || branch == "HEAD" {
        None
    } else {
        Some(branch)
    }
}
