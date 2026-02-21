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
