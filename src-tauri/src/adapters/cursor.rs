use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::adapter::AgentAdapter;
use crate::models::{AgentSession, AgentStatus, AgentType};
use crate::process::ProcessDetector;
use crate::util::detect_git_branch;

// --- Parsed transcript data ---

struct CursorTranscriptData {
    session_id: String,
    first_user_query: Option<String>,
    message_count: u32,
    last_turn_type: Option<String>,
}

// --- Adapter ---

pub struct CursorAdapter {
    projects_dir: PathBuf,
    process_detector: ProcessDetector,
}

impl CursorAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            projects_dir: home.join(".cursor").join("projects"),
            process_detector: ProcessDetector::new(),
        }
    }

    fn is_cursor_running(&self) -> bool {
        Command::new("pgrep")
            .args(["-x", "Cursor"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Decode a Cursor project slug like "Users-trevorstenson-Development-notchai"
    /// into "/Users/trevorstenson/Development/notchai" by greedily matching
    /// filesystem paths (handles hyphens in directory names).
    fn decode_project_slug(slug: &str) -> Option<String> {
        if slug.is_empty() || slug.starts_with('.') {
            return None;
        }

        let parts: Vec<&str> = slug.split('-').collect();
        if parts.is_empty() {
            return None;
        }

        let mut current = PathBuf::from("/");
        let mut i = 0;

        while i < parts.len() {
            let mut found = false;
            // Try longest match first to handle hyphens in directory names
            for end in (i + 1..=parts.len()).rev() {
                let candidate = parts[i..end].join("-");
                let test_path = current.join(&candidate);
                if test_path.exists() {
                    current = test_path;
                    i = end;
                    found = true;
                    break;
                }
            }
            if !found {
                // No existing path found, use single component
                current = current.join(parts[i]);
                i += 1;
            }
        }

        Some(current.to_string_lossy().to_string())
    }

    fn scan_transcripts(&self) -> Vec<(String, String, Vec<PathBuf>)> {
        // Returns: Vec<(project_path, project_name, transcript_files)>
        if !self.projects_dir.exists() {
            return Vec::new();
        }

        let cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(7 * 24 * 60 * 60))
            .unwrap_or(UNIX_EPOCH);

        let mut results = Vec::new();

        let projects = match fs::read_dir(&self.projects_dir) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };

        for project_entry in projects.flatten() {
            let project_dir = project_entry.path();
            if !project_dir.is_dir() {
                continue;
            }

            let slug = match project_dir.file_name().and_then(|n| n.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };

            let transcripts_dir = project_dir.join("agent-transcripts");
            if !transcripts_dir.is_dir() {
                continue;
            }

            let project_path = match Self::decode_project_slug(&slug) {
                Some(p) => p,
                None => continue,
            };

            let project_name = Path::new(&project_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let mut transcript_files = Vec::new();

            let entries = match fs::read_dir(&transcripts_dir) {
                Ok(d) => d,
                Err(_) => continue,
            };

            for file_entry in entries.flatten() {
                let file_path = file_entry.path();
                let name = file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if !name.ends_with(".txt") {
                    continue;
                }
                if let Ok(meta) = fs::metadata(&file_path) {
                    if let Ok(modified) = meta.modified() {
                        if modified >= cutoff {
                            transcript_files.push(file_path);
                        }
                    }
                }
            }

            if !transcript_files.is_empty() {
                results.push((project_path, project_name, transcript_files));
            }
        }

        results
    }

    fn parse_transcript(path: &Path) -> Option<CursorTranscriptData> {
        let session_id = path
            .file_stem()
            .and_then(|n| n.to_str())?
            .to_string();

        // Read up to 1MB to avoid blocking on huge transcripts
        let file = fs::File::open(path).ok()?;
        let meta = file.metadata().ok()?;
        let file_size = meta.len();
        let max_bytes = 1_048_576u64; // 1MB

        let content = if file_size <= max_bytes {
            let mut reader = BufReader::new(file);
            let mut content = String::new();
            reader.read_to_string(&mut content).ok()?;
            content
        } else {
            // Read first 512KB and last 512KB
            let half = (max_bytes / 2) as usize;
            let mut reader = BufReader::new(&file);
            let mut first_half = vec![0u8; half];
            reader.read_exact(&mut first_half).ok()?;

            let mut last_half = vec![0u8; half];
            let file2 = fs::File::open(path).ok()?;
            let mut reader2 = BufReader::new(file2);
            use std::io::Seek;
            reader2
                .seek(std::io::SeekFrom::End(-(half as i64)))
                .ok()?;
            reader2.read_exact(&mut last_half).ok()?;

            let first_str = String::from_utf8_lossy(&first_half).to_string();
            let last_str = String::from_utf8_lossy(&last_half).to_string();
            format!("{}\n{}", first_str, last_str)
        };

        let mut user_count: u32 = 0;
        let mut assistant_count: u32 = 0;
        let mut first_user_query: Option<String> = None;
        let mut last_turn_type: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == "user:" {
                user_count += 1;
                last_turn_type = Some("user".to_string());
            } else if trimmed == "A:" {
                assistant_count += 1;
                last_turn_type = Some("assistant".to_string());
            }
        }

        // Extract first <user_query>...</user_query>
        if let Some(start) = content.find("<user_query>") {
            let after_tag = start + "<user_query>".len();
            if let Some(end) = content[after_tag..].find("</user_query>") {
                let query = content[after_tag..after_tag + end].trim();
                first_user_query = Some(query.chars().take(200).collect());
            }
        }

        Some(CursorTranscriptData {
            session_id,
            first_user_query,
            message_count: user_count + assistant_count,
            last_turn_type,
        })
    }

    fn resolve_status(
        is_cursor_running: bool,
        file_age_secs: Option<u64>,
        last_turn_type: Option<&str>,
    ) -> AgentStatus {
        const OPERATING_WINDOW_SECS: u64 = 10;
        const ACTIVE_WINDOW_SECS: u64 = 900;

        let age = file_age_secs.unwrap_or(u64::MAX);

        if !is_cursor_running || age > ACTIVE_WINDOW_SECS {
            return AgentStatus::Completed;
        }

        if age < OPERATING_WINDOW_SECS {
            return AgentStatus::Operating;
        }

        match last_turn_type {
            Some("assistant") => AgentStatus::WaitingForInput,
            _ => AgentStatus::Idle,
        }
    }

    fn file_time_rfc3339(path: &Path, use_created: bool) -> String {
        fs::metadata(path)
            .ok()
            .and_then(|m| {
                if use_created {
                    m.created().ok()
                } else {
                    m.modified().ok()
                }
            })
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

impl AgentAdapter for CursorAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Cursor
    }

    fn name(&self) -> &str {
        "Cursor"
    }

    fn get_sessions(&self) -> Vec<AgentSession> {
        let projects = self.scan_transcripts();
        let is_cursor_running = self.is_cursor_running();
        let mut total_transcripts = 0;

        let mut sessions: Vec<AgentSession> = projects
            .iter()
            .flat_map(|(project_path, project_name, transcript_files)| {
                total_transcripts += transcript_files.len();

                transcript_files.iter().filter_map(|path| {
                    let data = Self::parse_transcript(path)?;
                    let file_age =
                        self.process_detector.get_jsonl_age_secs(&path.to_string_lossy());
                    let status = Self::resolve_status(
                        is_cursor_running,
                        file_age,
                        data.last_turn_type.as_deref(),
                    );

                    let git_branch = detect_git_branch(project_path).unwrap_or_default();
                    let created = Self::file_time_rfc3339(path, true);
                    let modified = Self::file_time_rfc3339(path, false);

                    Some(AgentSession {
                        agent_type: AgentType::Cursor,
                        id: data.session_id,
                        project_path: project_path.clone(),
                        project_name: project_name.clone(),
                        session_folder_path: project_path.clone(),
                        session_folder_name: project_name.clone(),
                        git_branch,
                        first_prompt: data.first_user_query.unwrap_or_default(),
                        summary: None,
                        created,
                        modified,
                        status,
                        message_count: data.message_count,
                        total_input_tokens: 0,
                        total_output_tokens: 0,
                        current_task: None,
                        model: None,
                        is_sidechain: false,
                    })
                })
            })
            .collect();

        eprintln!(
            "[notchai] cursor adapter: projects={} transcripts={} cursor_running={} sessions={}",
            projects.len(),
            total_transcripts,
            is_cursor_running,
            sessions.len()
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
