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
    #[allow(dead_code)]
    pub timestamp: Option<String>,
    pub message: Option<TranscriptMessage>,
}

#[derive(Debug, Deserialize)]
pub struct TranscriptMessage {
    #[allow(dead_code)]
    pub role: Option<String>,
    pub model: Option<String>,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
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
