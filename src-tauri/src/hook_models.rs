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
    /// JSON string of permission_suggestions from Claude Code PermissionRequest events.
    pub permission_suggestions: Option<String>,
    /// Title from Notification hook events.
    pub title: Option<String>,
    /// Message body from Notification hook events.
    pub message: Option<String>,
}

/// Payload emitted as a Tauri event for Notification hook events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPayload {
    pub session_id: String,
    pub title: String,
    pub message: String,
    pub timestamp: String,
}

/// Decision sent back to the hook script for permission requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionDecision {
    pub decision: String,
    pub reason: Option<String>,
    /// For AskUserQuestion: JSON string of updatedInput to pass back via the hook.
    pub updated_input: Option<String>,
    /// For "always allow": JSON string of updatedPermissions array to pass back via the hook.
    pub updated_permissions: Option<String>,
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
    /// JSON string of permission_suggestions (e.g. [{"type":"toolAlwaysAllow","tool":"Bash"}]).
    pub permission_suggestions: Option<String>,
}
