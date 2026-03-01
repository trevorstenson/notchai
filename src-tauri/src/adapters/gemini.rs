use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::adapter::AgentAdapter;
use crate::models::{
    AgentSession, AgentStatus, AgentType, GeminiConversationRecord, ToolCallInfo,
};
use crate::process::ProcessDetector;
use crate::util::detect_git_branch;

#[derive(Clone)]
struct GeminiSessionData {
    id: String,
    project_hash: String,
    model: Option<String>,
    first_prompt: Option<String>,
    message_count: u32,
    total_input_tokens: u64,
    total_output_tokens: u64,
}

pub struct GeminiAdapter {
    process_detector: ProcessDetector,
    mtime_cache: Mutex<HashMap<String, SystemTime>>,
    session_cache: Mutex<HashMap<String, GeminiSessionData>>,
}

impl GeminiAdapter {
    pub fn new() -> Self {
        Self {
            process_detector: ProcessDetector::new(),
            mtime_cache: Mutex::new(HashMap::new()),
            session_cache: Mutex::new(HashMap::new()),
        }
    }

    fn gemini_home(&self) -> PathBuf {
        if let Ok(val) = std::env::var("GEMINI_CLI_HOME") {
            return PathBuf::from(val);
        }
        dirs::home_dir()
            .unwrap_or_default()
            .join(".gemini")
    }

    fn is_gemini_running(&self) -> bool {
        let output = match Command::new("ps").args(["-eo", "pid,args"]).output() {
            Ok(o) => o,
            Err(_) => return false,
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().any(|line| {
            let line = line.trim();
            (line.contains("gemini-cli")
                || line.contains("/gemini ")
                || line.ends_with("/gemini")
                || line.ends_with(" gemini"))
                && !line.contains("grep")
                && !line.contains("notchai")
        })
    }

    fn scan_session_files(&self) -> Vec<(PathBuf, String)> {
        let tmp_dir = self.gemini_home().join("tmp");
        if !tmp_dir.exists() {
            return Vec::new();
        }

        let cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(7 * 24 * 60 * 60))
            .unwrap_or(UNIX_EPOCH);

        let mut files: Vec<(PathBuf, String)> = Vec::new();

        let project_dirs = match fs::read_dir(&tmp_dir) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };

        for project_entry in project_dirs.flatten() {
            let project_path = project_entry.path();
            if !project_path.is_dir() {
                continue;
            }

            let project_hash = project_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let session_files = match fs::read_dir(&project_path) {
                Ok(d) => d,
                Err(_) => continue,
            };

            for file_entry in session_files.flatten() {
                let file_path = file_entry.path();
                let name = file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                if !name.starts_with("session-") || !name.ends_with(".json") {
                    continue;
                }

                if let Ok(meta) = fs::metadata(&file_path) {
                    if let Ok(modified) = meta.modified() {
                        if modified >= cutoff {
                            files.push((file_path, project_hash.clone()));
                        }
                    }
                }
            }
        }

        files.sort_by(|a, b| {
            let a_mod = fs::metadata(&a.0)
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH);
            let b_mod = fs::metadata(&b.0)
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH);
            b_mod.cmp(&a_mod)
        });

        files
    }

    fn parse_session(&self, path: &Path, project_hash: &str) -> Option<GeminiSessionData> {
        let content = fs::read_to_string(path).ok()?;
        let record: GeminiConversationRecord = serde_json::from_str(&content).ok()?;

        let messages = record.messages.unwrap_or_default();

        let session_id = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mut model: Option<String> = None;
        let mut first_prompt: Option<String> = None;
        let mut message_count: u32 = 0;
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;

        for msg in &messages {
            let role = msg.role.as_deref().unwrap_or("");
            let msg_type = msg.message_type.as_deref().unwrap_or("");

            if role == "user" {
                message_count += 1;
                if first_prompt.is_none() {
                    first_prompt = msg.content.clone();
                }
            } else if msg_type == "gemini" || role == "assistant" || role == "model" {
                message_count += 1;
                if model.is_none() {
                    model = msg.model.clone();
                }
            }

            // Sum tokens from gemini-type messages
            if let Some(tokens) = &msg.tokens {
                total_input_tokens += tokens.input.unwrap_or(0);
                total_output_tokens += tokens.output.unwrap_or(0);
            }
        }

        Some(GeminiSessionData {
            id: session_id,
            project_hash: project_hash.to_string(),
            model,
            first_prompt,
            message_count,
            total_input_tokens,
            total_output_tokens,
        })
    }

    fn parse_session_cached(&self, path: &Path, project_hash: &str) -> Option<GeminiSessionData> {
        let key = path.to_string_lossy().to_string();

        let current_mtime = fs::metadata(path).ok()?.modified().ok()?;

        let mut mtime_cache = self.mtime_cache.lock().unwrap();
        let mut session_cache = self.session_cache.lock().unwrap();

        if let Some(cached_mtime) = mtime_cache.get(&key) {
            if *cached_mtime == current_mtime {
                return session_cache.get(&key).cloned();
            }
        }

        // mtime changed or first read — parse the file
        let data = self.parse_session(path, project_hash)?;

        mtime_cache.insert(key.clone(), current_mtime);
        session_cache.insert(key, data.clone());

        Some(data)
    }

    fn resolve_status(
        is_gemini_running: bool,
        file_age_secs: Option<u64>,
    ) -> AgentStatus {
        const OPERATING_WINDOW_SECS: u64 = 10;
        const ACTIVE_WINDOW_SECS: u64 = 900;

        let age = file_age_secs.unwrap_or(u64::MAX);

        if !is_gemini_running || age > ACTIVE_WINDOW_SECS {
            return AgentStatus::Completed;
        }

        if age < OPERATING_WINDOW_SECS {
            return AgentStatus::Operating;
        }

        AgentStatus::Idle
    }

    fn find_session_path(&self, session_id: &str) -> Option<PathBuf> {
        let files = self.scan_session_files();
        for (path, _) in files {
            let stem = path.file_stem().and_then(|n| n.to_str()).unwrap_or("");
            if stem == session_id {
                return Some(path);
            }
        }
        None
    }

    fn parse_tool_calls_from_session(path: &Path) -> Vec<ToolCallInfo> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let record: GeminiConversationRecord = match serde_json::from_str(&content) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        let messages = record.messages.unwrap_or_default();
        let mut calls: Vec<ToolCallInfo> = Vec::new();
        let mut call_index: u32 = 0;

        for msg in &messages {
            let msg_type = msg.message_type.as_deref().unwrap_or("");
            let role = msg.role.as_deref().unwrap_or("");

            if msg_type != "gemini" && role != "assistant" && role != "model" {
                continue;
            }

            if let Some(tool_calls) = &msg.tool_calls {
                for tc in tool_calls {
                    let name = tc.name.as_deref().unwrap_or("unknown").to_string();
                    let input_summary = tc
                        .input
                        .as_ref()
                        .map(|v| {
                            let s = match v {
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
                                _ => v.to_string(),
                            };
                            if s.len() > 200 {
                                format!("{}...", &s[..197])
                            } else {
                                s
                            }
                        })
                        .unwrap_or_default();

                    calls.push(ToolCallInfo {
                        id: format!("gemini-tc-{}", call_index),
                        tool_name: name.clone(),
                        display_name: name,
                        input_summary,
                        status: "success".to_string(),
                        timestamp: None,
                        duration_ms: None,
                        result_preview: None,
                    });

                    call_index += 1;
                }
            }
        }

        // Return last 20
        let len = calls.len();
        if len > 20 {
            calls.drain(..len - 20);
        }

        calls
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::GeminiConversationRecord;

    #[test]
    fn test_gemini_conversation_record_parsing() {
        let json = r#"{
            "messages": [
                {"role": "user", "content": "Help me write a function"},
                {"type": "gemini", "role": "model", "model": "gemini-2.0-pro", "content": "Here's a function...", "tokens": {"input": 50, "output": 100}},
                {"role": "user", "content": "Now add tests"},
                {"type": "gemini", "role": "model", "model": "gemini-2.0-pro", "content": "Here are the tests...", "tokens": {"input": 150, "output": 200}}
            ]
        }"#;

        let record: GeminiConversationRecord = serde_json::from_str(json).unwrap();
        let messages = record.messages.unwrap();
        assert_eq!(messages.len(), 4);

        assert_eq!(messages[0].role.as_deref(), Some("user"));
        assert_eq!(
            messages[0].content.as_deref(),
            Some("Help me write a function")
        );

        assert_eq!(messages[1].message_type.as_deref(), Some("gemini"));
        assert_eq!(messages[1].model.as_deref(), Some("gemini-2.0-pro"));

        let tokens = messages[1].tokens.as_ref().unwrap();
        assert_eq!(tokens.input, Some(50));
        assert_eq!(tokens.output, Some(100));

        let tokens2 = messages[3].tokens.as_ref().unwrap();
        assert_eq!(tokens2.input, Some(150));
        assert_eq!(tokens2.output, Some(200));
    }

    #[test]
    fn test_gemini_tool_calls_extraction() {
        let json = r#"{
            "messages": [
                {"role": "user", "content": "Read file foo.txt"},
                {"type": "gemini", "role": "model", "content": "I'll read that file", "toolCalls": [
                    {"name": "read_file", "input": {"path": "foo.txt"}},
                    {"name": "write_file", "input": {"path": "bar.txt", "content": "hello"}}
                ]}
            ]
        }"#;

        let path = std::env::temp_dir().join("notchai_test_gemini_tools.json");
        std::fs::write(&path, json).unwrap();

        let calls = GeminiAdapter::parse_tool_calls_from_session(&path);

        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].tool_name, "read_file");
        assert_eq!(calls[0].id, "gemini-tc-0");
        assert!(calls[0].input_summary.contains("path"));
        assert_eq!(calls[1].tool_name, "write_file");
        assert_eq!(calls[1].id, "gemini-tc-1");
        assert!(calls[1].input_summary.contains("path"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_gemini_empty_messages() {
        let json = r#"{"messages": []}"#;
        let record: GeminiConversationRecord = serde_json::from_str(json).unwrap();
        let messages = record.messages.unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn test_gemini_null_messages() {
        let json = r#"{}"#;
        let record: GeminiConversationRecord = serde_json::from_str(json).unwrap();
        assert!(record.messages.is_none());
    }

    #[test]
    fn test_gemini_parse_session_counts() {
        let json = r#"{
            "messages": [
                {"role": "user", "content": "Hello"},
                {"type": "gemini", "role": "model", "model": "gemini-2.0-flash", "content": "Hi!", "tokens": {"input": 10, "output": 20}},
                {"role": "user", "content": "Help me"},
                {"type": "gemini", "role": "model", "content": "Sure!", "tokens": {"input": 30, "output": 40}}
            ]
        }"#;

        let path = std::env::temp_dir().join("notchai_test_gemini_session.json");
        std::fs::write(&path, json).unwrap();

        let adapter = GeminiAdapter {
            process_detector: ProcessDetector::new(),
            mtime_cache: Mutex::new(HashMap::new()),
            session_cache: Mutex::new(HashMap::new()),
        };

        let data = adapter.parse_session(&path, "test-hash").unwrap();

        assert_eq!(data.message_count, 4);
        assert_eq!(data.first_prompt, Some("Hello".to_string()));
        assert_eq!(data.model, Some("gemini-2.0-flash".to_string()));
        assert_eq!(data.total_input_tokens, 40);
        assert_eq!(data.total_output_tokens, 60);
        assert_eq!(data.project_hash, "test-hash");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_gemini_no_tool_calls() {
        let json = r#"{
            "messages": [
                {"role": "user", "content": "Just a question"},
                {"type": "gemini", "role": "model", "content": "Here's the answer"}
            ]
        }"#;

        let path = std::env::temp_dir().join("notchai_test_gemini_no_tools.json");
        std::fs::write(&path, json).unwrap();

        let calls = GeminiAdapter::parse_tool_calls_from_session(&path);
        assert!(calls.is_empty());

        let _ = std::fs::remove_file(&path);
    }
}

impl AgentAdapter for GeminiAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Gemini
    }

    fn name(&self) -> &str {
        "Gemini CLI"
    }

    fn get_tool_calls(&self, session_id: &str) -> Vec<ToolCallInfo> {
        match self.find_session_path(session_id) {
            Some(path) => Self::parse_tool_calls_from_session(&path),
            None => Vec::new(),
        }
    }

    fn get_sessions(&self) -> Vec<AgentSession> {
        let files = self.scan_session_files();
        let is_gemini_running = self.is_gemini_running();
        let total_files = files.len();

        let mut sessions: Vec<AgentSession> = files
            .iter()
            .filter_map(|(path, project_hash)| {
                let data = self.parse_session_cached(path, project_hash)?;
                let file_age = self.process_detector.get_jsonl_age_secs(&path.to_string_lossy());
                let status = Self::resolve_status(is_gemini_running, file_age);

                let first_prompt = data
                    .first_prompt
                    .unwrap_or_default()
                    .chars()
                    .take(200)
                    .collect();

                // Project name: first 8 chars of hash
                let project_name = if data.project_hash.len() >= 8 {
                    data.project_hash[..8].to_string()
                } else {
                    data.project_hash.clone()
                };

                let session_folder_path = path
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                let git_branch = detect_git_branch(&session_folder_path).unwrap_or_default();

                let modified = Self::file_modified_rfc3339(path);
                let created = modified.clone();

                Some(AgentSession {
                    agent_type: AgentType::Gemini,
                    id: data.id,
                    project_path: session_folder_path.clone(),
                    project_name,
                    session_folder_path: session_folder_path.clone(),
                    session_folder_name: data.project_hash,
                    git_branch,
                    first_prompt,
                    summary: None,
                    created,
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
            "[notchai] gemini adapter: total_files={} gemini_running={} sessions={}",
            total_files, is_gemini_running, sessions.len()
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
