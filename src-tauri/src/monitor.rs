use std::sync::Mutex;

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

        let mut reader = self.transcript_reader.lock().unwrap();

        let mut sessions: Vec<AgentSession> = entries
            .iter()
            .filter(|e| self.is_recent(e))
            .map(|entry| {
                let transcript_entries = reader.read_recent_entries(
                    &entry.session_id,
                    &entry.full_path,
                    50_000,
                );

                let (total_input, total_output) =
                    TranscriptReader::get_token_totals(&transcript_entries);
                let last_msg_type = TranscriptReader::get_last_message_type(&transcript_entries);
                let model = TranscriptReader::get_model(&transcript_entries);

                let jsonl_age = self.process_detector.get_jsonl_age_secs(&entry.full_path);
                let is_file_active = self.process_detector.is_session_active(&entry.full_path);

                let status = Self::resolve_status(
                    has_claude_running,
                    is_file_active,
                    jsonl_age,
                    last_msg_type.as_deref(),
                );

                let project_name = entry
                    .project_path
                    .as_deref()
                    .unwrap_or("unknown")
                    .rsplit('/')
                    .next()
                    .unwrap_or("unknown")
                    .to_string();

                AgentSession {
                    id: entry.session_id.clone(),
                    project_path: entry.project_path.clone().unwrap_or_default(),
                    project_name,
                    git_branch: entry.git_branch.clone().unwrap_or_default(),
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
                    total_input_tokens: total_input,
                    total_output_tokens: total_output,
                    current_task: None,
                    model,
                    is_sidechain: entry.is_sidechain.unwrap_or(false),
                }
            })
            .collect();

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
        if !has_claude_running || !is_file_active {
            return AgentStatus::Completed;
        }

        let age = jsonl_age.unwrap_or(u64::MAX);

        if age < 10 {
            return AgentStatus::Operating;
        }

        if age < 30 {
            return AgentStatus::Idle;
        }

        match last_msg_type {
            Some("assistant") => AgentStatus::WaitingForInput,
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
}
