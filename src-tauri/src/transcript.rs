use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

use crate::models::TranscriptEntry;

pub struct TranscriptReader {
    offsets: HashMap<String, u64>,
    telemetry: HashMap<String, SessionTelemetry>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionTelemetry {
    pub total_input: u64,
    pub total_output: u64,
    pub last_message_type: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
}

impl TranscriptReader {
    pub fn new() -> Self {
        Self {
            offsets: HashMap::new(),
            telemetry: HashMap::new(),
        }
    }

    pub fn read_recent_entries(
        &mut self,
        session_id: &str,
        file_path: &str,
        max_initial_bytes: u64,
    ) -> Vec<TranscriptEntry> {
        let path = Path::new(file_path);
        if !path.exists() {
            return Vec::new();
        }

        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let metadata = match file.metadata() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };

        let file_size = metadata.len();
        let start_offset = self.offsets.get(session_id).copied().unwrap_or(0);

        if start_offset >= file_size {
            return Vec::new();
        }

        // For initial read, only read last N bytes
        let read_from = if start_offset == 0 && file_size > max_initial_bytes {
            file_size - max_initial_bytes
        } else {
            start_offset
        };

        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(read_from)).is_err() {
            return Vec::new();
        }

        let mut entries = Vec::new();
        let mut line = String::new();

        // Skip partial first line if we seeked to middle
        if read_from > 0 && start_offset == 0 {
            let _ = reader.read_line(&mut line);
            line.clear();
        }

        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                if let Ok(entry) = serde_json::from_str::<TranscriptEntry>(trimmed) {
                    entries.push(entry);
                }
            }
            line.clear();
        }

        self.offsets.insert(session_id.to_string(), file_size);
        self.update_telemetry(session_id, &entries);
        entries
    }

    fn update_telemetry(&mut self, session_id: &str, entries: &[TranscriptEntry]) {
        if entries.is_empty() {
            return;
        }

        let state = self
            .telemetry
            .entry(session_id.to_string())
            .or_default();

        for entry in entries {
            if let Some(cwd) = entry.cwd.clone() {
                state.cwd = Some(cwd);
            }

            if let Some(ref msg) = entry.message {
                if let Some(ref usage) = msg.usage {
                    state.total_input += usage.input_tokens.unwrap_or(0);
                    state.total_input += usage.cache_creation_input_tokens.unwrap_or(0);
                    state.total_input += usage.cache_read_input_tokens.unwrap_or(0);
                    state.total_output += usage.output_tokens.unwrap_or(0);
                }
                if let Some(model) = msg.model.clone() {
                    state.model = Some(model);
                }
            }

            if matches!(
                entry.entry_type.as_deref(),
                Some("user") | Some("assistant")
            ) {
                state.last_message_type = entry.entry_type.clone();
            }
        }
    }

    pub fn get_telemetry(&self, session_id: &str) -> SessionTelemetry {
        self.telemetry
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_token_totals(entries: &[TranscriptEntry]) -> (u64, u64) {
        let mut total_input = 0u64;
        let mut total_output = 0u64;

        for entry in entries {
            if let Some(ref msg) = entry.message {
                if let Some(ref usage) = msg.usage {
                    total_input += usage.input_tokens.unwrap_or(0);
                    total_input += usage.cache_creation_input_tokens.unwrap_or(0);
                    total_input += usage.cache_read_input_tokens.unwrap_or(0);
                    total_output += usage.output_tokens.unwrap_or(0);
                }
            }
        }

        (total_input, total_output)
    }

    pub fn get_last_message_type(entries: &[TranscriptEntry]) -> Option<String> {
        entries
            .iter()
            .rev()
            .find(|e| {
                matches!(
                    e.entry_type.as_deref(),
                    Some("user") | Some("assistant")
                )
            })
            .and_then(|e| e.entry_type.clone())
    }

    pub fn get_model(entries: &[TranscriptEntry]) -> Option<String> {
        entries
            .iter()
            .rev()
            .find_map(|e| e.message.as_ref().and_then(|m| m.model.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_file(name: &str, lines: &[&str]) -> String {
        let path = std::env::temp_dir().join(format!("notchai_test_{}", name));
        let mut f = std::fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }
        path.to_string_lossy().to_string()
    }

    #[test]
    fn test_normal_conversation_two_messages() {
        let path = write_temp_file("transcript_normal", &[
            r#"{"type":"user","message":{"role":"user","content":[]},"cwd":"/tmp/project","timestamp":"2024-01-01T00:00:00Z"}"#,
            r#"{"type":"assistant","message":{"role":"assistant","model":"claude-3-opus","usage":{"input_tokens":100,"output_tokens":50},"content":[]},"timestamp":"2024-01-01T00:00:01Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":[]},"timestamp":"2024-01-01T00:00:02Z"}"#,
            r#"{"type":"assistant","message":{"role":"assistant","model":"claude-3-opus","usage":{"input_tokens":200,"output_tokens":75},"content":[]},"timestamp":"2024-01-01T00:00:03Z"}"#,
        ]);

        let mut reader = TranscriptReader::new();
        let entries = reader.read_recent_entries("test-normal", &path, 100_000);

        assert_eq!(entries.len(), 4);

        let (input, output) = TranscriptReader::get_token_totals(&entries);
        assert_eq!(input, 300);
        assert_eq!(output, 125);

        let last_type = TranscriptReader::get_last_message_type(&entries);
        assert_eq!(last_type, Some("assistant".to_string()));

        let model = TranscriptReader::get_model(&entries);
        assert_eq!(model, Some("claude-3-opus".to_string()));

        // Telemetry should be accumulated
        let telemetry = reader.get_telemetry("test-normal");
        assert_eq!(telemetry.total_input, 300);
        assert_eq!(telemetry.total_output, 125);
        assert_eq!(telemetry.model, Some("claude-3-opus".to_string()));
        assert_eq!(telemetry.cwd, Some("/tmp/project".to_string()));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_tool_use_and_tool_result_correlation() {
        let path = write_temp_file("transcript_tools", &[
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"tu-1","name":"Read","input":{"path":"foo.txt"}}]},"timestamp":"2024-01-01T00:00:00Z"}"#,
            r#"{"type":"tool_result","tool_use_id":"tu-1","duration_ms":150,"result":"file content","is_error":false,"timestamp":"2024-01-01T00:00:01Z"}"#,
        ]);

        let mut reader = TranscriptReader::new();
        let entries = reader.read_recent_entries("test-tools", &path, 100_000);

        assert_eq!(entries.len(), 2);

        // First entry: assistant message with tool_use content block
        let tool_use_entry = &entries[0];
        assert_eq!(tool_use_entry.entry_type.as_deref(), Some("assistant"));
        let content = tool_use_entry
            .message
            .as_ref()
            .unwrap()
            .content
            .as_ref()
            .unwrap();
        assert_eq!(content.len(), 1);
        match &content[0] {
            crate::models::TranscriptContentBlock::ToolUse { id, name, .. } => {
                assert_eq!(id, "tu-1");
                assert_eq!(name, "Read");
            }
            _ => panic!("Expected ToolUse content block"),
        }

        // Second entry: tool_result correlated by tool_use_id
        let tool_result = &entries[1];
        assert_eq!(tool_result.entry_type.as_deref(), Some("tool_result"));
        assert_eq!(tool_result.tool_use_id.as_deref(), Some("tu-1"));
        assert_eq!(tool_result.duration_ms, Some(150));
        assert_eq!(tool_result.is_error, Some(false));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_malformed_jsonl_lines_skipped() {
        let path = write_temp_file("transcript_malformed", &[
            r#"{"type":"user","message":{"role":"user","content":[]},"timestamp":"2024-01-01T00:00:00Z"}"#,
            r#"this is not valid json{{{{"#,
            r#"{"truncated": true"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[]},"timestamp":"2024-01-01T00:00:01Z"}"#,
        ]);

        let mut reader = TranscriptReader::new();
        let entries = reader.read_recent_entries("test-malformed", &path, 100_000);

        // Malformed lines should be silently skipped
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].entry_type.as_deref(), Some("user"));
        assert_eq!(entries[1].entry_type.as_deref(), Some("assistant"));

        let _ = std::fs::remove_file(&path);
    }
}
