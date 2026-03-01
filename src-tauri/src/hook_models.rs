use serde::{Deserialize, Serialize};

/// Message received from the Python hook script over Unix socket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookMessage {
    pub event_type: String,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<String>,
    pub tool_use_id: Option<String>,
    pub agent: Option<String>,
    pub timestamp: Option<String>,
}

/// Decision sent back to the hook script for permission requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionDecision {
    pub decision: String,
    pub reason: Option<String>,
    /// For AskUserQuestion: JSON string of updatedInput to pass back via the hook.
    pub updated_input: Option<String>,
}

/// Payload emitted as a Tauri event for status updates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookStatusPayload {
    pub event_type: String,
    pub session_id: String,
    pub cwd: Option<String>,
    pub tool_name: Option<String>,
    pub agent: Option<String>,
    pub timestamp: String,
}

/// Payload emitted as a Tauri event for permission requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestPayload {
    pub request_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub tool_input: Option<String>,
    pub cwd: Option<String>,
    pub agent: Option<String>,
    pub timestamp: String,
    /// True when this is a PermissionRequest for AskUserQuestion (question card flow).
    pub is_question: bool,
}
