use serde::{Deserialize, Serialize};

// === Models sent to the frontend ===

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AgentType {
    Claude,
    Codex,
    Cursor,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AgentStatus {
    Operating,
    Idle,
    WaitingForInput,
    WaitingForApproval,
    Error,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingToolInfo {
    pub request_id: String,
    pub tool_name: String,
    pub tool_input_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallInfo {
    pub id: String,
    pub tool_name: String,
    pub display_name: String,
    pub input_summary: String,
    pub status: String,
    pub timestamp: Option<String>,
    pub duration_ms: Option<u64>,
    pub result_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub agent_type: AgentType,
    pub id: String,
    pub project_path: String,
    pub project_name: String,
    pub session_folder_path: String,
    pub session_folder_name: String,
    pub git_branch: String,
    pub first_prompt: String,
    pub summary: Option<String>,
    pub created: String,
    pub modified: String,
    pub status: AgentStatus,
    pub message_count: u32,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub current_task: Option<String>,
    pub model: Option<String>,
    pub is_sidechain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotchInfo {
    pub exists: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub screen_width: f64,
    pub screen_height: f64,
}

impl NotchInfo {
    pub fn no_notch(screen_width: f64, screen_height: f64) -> Self {
        let width = 200.0;
        Self {
            exists: false,
            x: (screen_width - width) / 2.0,
            y: 0.0,
            width,
            height: 32.0,
            screen_width,
            screen_height,
        }
    }

    pub fn center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenInfo {
    pub index: usize,
    pub name: String,
    pub has_notch: bool,
    pub width: f64,
    pub height: f64,
    pub is_primary: bool,
}

// === Models for parsing on-disk data ===

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIndexFile {
    #[allow(dead_code)]
    pub version: Option<u32>,
    pub entries: Vec<SessionIndexEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIndexEntry {
    pub session_id: String,
    pub full_path: String,
    pub first_prompt: Option<String>,
    pub summary: Option<String>,
    pub message_count: Option<u32>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub git_branch: Option<String>,
    pub project_path: Option<String>,
    pub is_sidechain: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct TranscriptEntry {
    #[serde(rename = "type")]
    pub entry_type: Option<String>,
    pub cwd: Option<String>,
    #[serde(rename = "sessionId")]
    #[allow(dead_code)]
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    pub message: Option<TranscriptMessage>,
    // Fields for top-level tool_result entries
    pub tool_use_id: Option<String>,
    pub duration_ms: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub is_error: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct TranscriptMessage {
    #[allow(dead_code)]
    pub role: Option<String>,
    pub model: Option<String>,
    pub usage: Option<TokenUsage>,
    pub content: Option<Vec<TranscriptContentBlock>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum TranscriptContentBlock {
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: Option<serde_json::Value>,
        is_error: Option<bool>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
}

// === Unified event pipeline models ===

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum EventSource {
    Otel,
    Hook,
    Notify,
    Poll,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum NormalizedEvent {
    SessionStarted {
        agent_type: AgentType,
        session_id: String,
        timestamp: String,
        source: EventSource,
    },
    SessionEnded {
        agent_type: AgentType,
        session_id: String,
        timestamp: String,
        source: EventSource,
    },
    ToolStarted {
        agent_type: AgentType,
        session_id: String,
        timestamp: String,
        source: EventSource,
        tool_name: String,
        tool_input: Option<String>,
    },
    ToolCompleted {
        agent_type: AgentType,
        session_id: String,
        timestamp: String,
        source: EventSource,
        tool_name: String,
        status: String,
        duration_ms: Option<u64>,
        result_preview: Option<String>,
    },
    StatusChanged {
        agent_type: AgentType,
        session_id: String,
        timestamp: String,
        source: EventSource,
        new_status: AgentStatus,
    },
    TokensUsed {
        agent_type: AgentType,
        session_id: String,
        timestamp: String,
        source: EventSource,
        input_tokens: u64,
        output_tokens: u64,
    },
    PermissionRequested {
        agent_type: AgentType,
        session_id: String,
        timestamp: String,
        source: EventSource,
        tool_name: String,
        tool_input: Option<String>,
        request_id: String,
    },
    TaskCompleted {
        agent_type: AgentType,
        session_id: String,
        timestamp: String,
        source: EventSource,
    },
    Error {
        agent_type: AgentType,
        session_id: String,
        timestamp: String,
        source: EventSource,
        message: String,
    },
}

// === Gemini CLI session data models ===

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiConversationRecord {
    pub messages: Option<Vec<GeminiMessageRecord>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiMessageRecord {
    #[serde(rename = "type")]
    pub message_type: Option<String>,
    pub role: Option<String>,
    pub content: Option<String>,
    pub model: Option<String>,
    pub tokens: Option<GeminiTokensSummary>,
    pub tool_calls: Option<Vec<GeminiToolCallRecord>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiToolCallRecord {
    pub name: Option<String>,
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiTokensSummary {
    pub input: Option<u64>,
    pub output: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalized_event_serde_roundtrip_session_started() {
        let event = NormalizedEvent::SessionStarted {
            agent_type: AgentType::Claude,
            session_id: "sess-123".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            source: EventSource::Otel,
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: NormalizedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_normalized_event_serde_roundtrip_tool_completed() {
        let event = NormalizedEvent::ToolCompleted {
            agent_type: AgentType::Codex,
            session_id: "sess-456".to_string(),
            timestamp: "2024-01-01T00:00:01Z".to_string(),
            source: EventSource::Hook,
            tool_name: "Read".to_string(),
            status: "success".to_string(),
            duration_ms: Some(150),
            result_preview: Some("file content...".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: NormalizedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_normalized_event_serde_roundtrip_tokens_used() {
        let event = NormalizedEvent::TokensUsed {
            agent_type: AgentType::Gemini,
            session_id: "sess-789".to_string(),
            timestamp: "2024-01-01T00:00:02Z".to_string(),
            source: EventSource::Poll,
            input_tokens: 1000,
            output_tokens: 500,
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: NormalizedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_normalized_event_serde_roundtrip_error() {
        let event = NormalizedEvent::Error {
            agent_type: AgentType::Cursor,
            session_id: "sess-err".to_string(),
            timestamp: "2024-01-01T00:00:03Z".to_string(),
            source: EventSource::Notify,
            message: "Something went wrong".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: NormalizedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_normalized_event_serde_roundtrip_all_variants() {
        let events = vec![
            NormalizedEvent::SessionStarted {
                agent_type: AgentType::Claude,
                session_id: "s1".to_string(),
                timestamp: "t1".to_string(),
                source: EventSource::Otel,
            },
            NormalizedEvent::SessionEnded {
                agent_type: AgentType::Codex,
                session_id: "s2".to_string(),
                timestamp: "t2".to_string(),
                source: EventSource::Hook,
            },
            NormalizedEvent::ToolStarted {
                agent_type: AgentType::Claude,
                session_id: "s3".to_string(),
                timestamp: "t3".to_string(),
                source: EventSource::Otel,
                tool_name: "Bash".to_string(),
                tool_input: Some("ls -la".to_string()),
            },
            NormalizedEvent::StatusChanged {
                agent_type: AgentType::Gemini,
                session_id: "s4".to_string(),
                timestamp: "t4".to_string(),
                source: EventSource::Poll,
                new_status: AgentStatus::Operating,
            },
            NormalizedEvent::PermissionRequested {
                agent_type: AgentType::Claude,
                session_id: "s5".to_string(),
                timestamp: "t5".to_string(),
                source: EventSource::Hook,
                tool_name: "Write".to_string(),
                tool_input: Some("content".to_string()),
                request_id: "req-1".to_string(),
            },
            NormalizedEvent::TaskCompleted {
                agent_type: AgentType::Codex,
                session_id: "s6".to_string(),
                timestamp: "t6".to_string(),
                source: EventSource::Notify,
            },
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let deserialized: NormalizedEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, deserialized);
        }
    }
}
