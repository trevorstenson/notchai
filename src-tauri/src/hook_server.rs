use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde_json;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{oneshot, Mutex};

use crate::event_bus::EventBus;
use crate::hook_models::{
    HookMessage, HookStatusPayload, NotificationPayload, PermissionDecision,
    PermissionRequestPayload,
};
use crate::models::{AgentStatus, AgentType, EventSource, NormalizedEvent};

/// Time-to-live for pending approval requests (5 minutes).
const APPROVAL_TTL_SECS: u64 = 300;

static HOOK_SERVER: OnceLock<HookServer> = OnceLock::new();

/// Returns the global HookServer instance, if started.
pub fn get_server() -> Option<&'static HookServer> {
    HOOK_SERVER.get()
}

pub struct HookServer {
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<PermissionDecision>>>>,
    /// Cache tool_use_id from PreToolUse events keyed by (session_id, tool_name).
    /// PermissionRequest events don't include tool_use_id, so we correlate from the
    /// most recent PreToolUse for the same session+tool.
    tool_use_id_cache: Arc<Mutex<HashMap<(String, String), String>>>,
    /// Dedup map: (session_id, tool_name, tool_use_id) → request_id.
    /// Used to detect and cancel duplicate pending approvals.
    dedup_map: Arc<Mutex<HashMap<(String, String, String), String>>>,
}

impl HookServer {
    fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            tool_use_id_cache: Arc::new(Mutex::new(HashMap::new())),
            dedup_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Respond to a pending permission request by request_id.
    pub async fn respond(&self, request_id: &str, decision: PermissionDecision) -> Result<(), String> {
        let sender = {
            let mut pending = self.pending.lock().await;
            pending.remove(request_id)
        };
        match sender {
            Some(tx) => {
                tx.send(decision).map_err(|_| "receiver dropped".to_string())
            }
            None => Err(format!("no pending request with id: {}", request_id)),
        }
    }
}

const SOCKET_PATH: &str = "/tmp/notchai.sock";

/// Start the Unix socket server. Should be called once during app setup.
pub async fn start(app: AppHandle, event_bus: EventBus) {
    // Initialize the global server instance
    let server = HookServer::new();
    let pending = server.pending.clone();
    let tool_use_id_cache = server.tool_use_id_cache.clone();
    let dedup_map = server.dedup_map.clone();
    let _ = HOOK_SERVER.set(server);

    // Remove stale socket file if it exists
    let _ = std::fs::remove_file(SOCKET_PATH);

    let listener = match UnixListener::bind(SOCKET_PATH) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[hook_server] failed to bind {}: {}", SOCKET_PATH, e);
            return;
        }
    };

    eprintln!("[hook_server] listening on {}", SOCKET_PATH);

    loop {
        let (stream, _addr) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("[hook_server] accept error: {}", e);
                continue;
            }
        };

        let app_handle = app.clone();
        let pending = pending.clone();
        let cache = tool_use_id_cache.clone();
        let dedup = dedup_map.clone();
        let bus = event_bus.clone();

        tokio::spawn(async move {
            let (reader, mut writer) = tokio::io::split(stream);
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();

            match buf_reader.read_line(&mut line).await {
                Ok(0) => return, // EOF
                Ok(_) => {}
                Err(e) => {
                    eprintln!("[hook_server] read error: {}", e);
                    return;
                }
            }

            let msg: HookMessage = match serde_json::from_str(line.trim()) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("[hook_server] malformed JSON: {}", e);
                    return;
                }
            };

            let session_id = msg.session_id.clone().unwrap_or_default();
            let timestamp = msg.timestamp.clone().unwrap_or_default();

            if msg.event_type == "PermissionRequest" {
                // Generate a unique request_id
                let request_id = format!(
                    "perm-{}-{}",
                    &session_id,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                );

                // Try to get cached tool_use_id from a recent PreToolUse event
                let tool_name = msg.tool_name.clone().unwrap_or_default();
                let tool_use_id = {
                    let cache_lock = cache.lock().await;
                    cache_lock
                        .get(&(session_id.clone(), tool_name.clone()))
                        .cloned()
                        .unwrap_or_default()
                };

                // Dedup: cancel any existing pending approval with the same
                // (session_id, tool_name, tool_use_id) composite key.
                let dedup_key = (session_id.clone(), tool_name.clone(), tool_use_id);
                {
                    let mut dedup_lock = dedup.lock().await;
                    if let Some(old_request_id) = dedup_lock.remove(&dedup_key) {
                        let mut pending_lock = pending.lock().await;
                        // Dropping the old sender cancels the oneshot (Err branch in select!)
                        if let Some(_old_tx) = pending_lock.remove(&old_request_id) {
                            eprintln!(
                                "[hook_server] dedup: replacing pending approval {} with {}",
                                old_request_id, request_id
                            );
                            let _ = app_handle.emit("hook:permission-cancelled", &old_request_id);
                        }
                    }
                    dedup_lock.insert(dedup_key.clone(), request_id.clone());
                }

                let is_question = tool_name == "AskUserQuestion";
                let payload = PermissionRequestPayload {
                    request_id: request_id.clone(),
                    session_id: session_id.clone(),
                    tool_name,
                    tool_input: msg.tool_input.clone(),
                    cwd: msg.cwd.clone(),
                    agent: msg.agent.clone(),
                    timestamp,
                    is_question,
                    permission_suggestions: msg.permission_suggestions.clone(),
                };

                let _ = app_handle.emit("hook:permission-request", &payload);

                // Create a oneshot channel and store the sender
                let (tx, rx) = oneshot::channel::<PermissionDecision>();
                {
                    let mut pending_lock = pending.lock().await;
                    pending_lock.insert(request_id.clone(), tx);
                }

                // Race: wait for UI response, detect hook script disconnect, or TTL expiry.
                let disconnect = async {
                    let mut discard = [0u8; 1];
                    loop {
                        match buf_reader.read(&mut discard).await {
                            Ok(0) => break,  // EOF — client disconnected
                            Err(_) => break, // Error — treat as disconnect
                            Ok(_) => {}      // Unexpected data, keep reading
                        }
                    }
                };

                let ttl = tokio::time::sleep(Duration::from_secs(APPROVAL_TTL_SECS));

                tokio::select! {
                    result = rx => {
                        // Clean up dedup entry on resolution
                        {
                            let mut dedup_lock = dedup.lock().await;
                            dedup_lock.remove(&dedup_key);
                        }
                        match result {
                            Ok(decision) => {
                                let mut response = serde_json::json!({
                                    "decision": decision.decision,
                                    "reason": decision.reason
                                });
                                if let Some(ref updated_input) = decision.updated_input {
                                    response["updated_input"] = serde_json::Value::String(updated_input.clone());
                                }
                                if let Some(ref updated_permissions) = decision.updated_permissions {
                                    response["updated_permissions"] = serde_json::Value::String(updated_permissions.clone());
                                }
                                let response_str = response.to_string() + "\n";
                                if let Err(e) = writer.write_all(response_str.as_bytes()).await {
                                    eprintln!("[hook_server] write response error: {}", e);
                                }
                            }
                            Err(_) => {
                                // Sender was dropped (e.g., dedup replaced it or server shutting down)
                                eprintln!("[hook_server] permission request {} cancelled", request_id);
                            }
                        }
                    }
                    _ = disconnect => {
                        // Hook script disconnected — clean up pending approval
                        eprintln!("[hook_server] hook client disconnected for {}, dismissing approval", request_id);
                        {
                            let mut pending_lock = pending.lock().await;
                            pending_lock.remove(&request_id);
                        }
                        {
                            let mut dedup_lock = dedup.lock().await;
                            dedup_lock.remove(&dedup_key);
                        }
                        let _ = app_handle.emit("hook:permission-cancelled", &request_id);
                    }
                    _ = ttl => {
                        // TTL expired — send deny and clean up
                        eprintln!("[hook_server] approval TTL expired for {}, auto-denying", request_id);
                        {
                            let mut pending_lock = pending.lock().await;
                            pending_lock.remove(&request_id);
                        }
                        {
                            let mut dedup_lock = dedup.lock().await;
                            dedup_lock.remove(&dedup_key);
                        }
                        // Send deny response to the hook script
                        let deny_response = serde_json::json!({
                            "decision": "deny",
                            "reason": "approval request timed out after 5 minutes"
                        });
                        let response_str = deny_response.to_string() + "\n";
                        let _ = writer.write_all(response_str.as_bytes()).await;
                        let _ = app_handle.emit("hook:permission-cancelled", &request_id);
                    }
                }
            } else {
                // Cache tool_use_id from PreToolUse events
                if msg.event_type == "PreToolUse" {
                    if let Some(ref tool_use_id) = msg.tool_use_id {
                        let tool_name = msg.tool_name.clone().unwrap_or_default();
                        if !session_id.is_empty() && !tool_name.is_empty() {
                            let mut cache_lock = cache.lock().await;
                            cache_lock.insert(
                                (session_id.clone(), tool_name),
                                tool_use_id.clone(),
                            );
                        }
                    }
                }

                // Emit status update for non-permission events (backwards compat)
                if !session_id.is_empty() {
                    let payload = HookStatusPayload {
                        event_type: msg.event_type.clone(),
                        session_id: session_id.clone(),
                        cwd: msg.cwd.clone(),
                        tool_name: msg.tool_name.clone(),
                        agent: msg.agent.clone(),
                        timestamp: timestamp.clone(),
                    };
                    let _ = app_handle.emit("hook:status-update", &payload);
                }

                // Handle Notification events: trigger macOS notification and emit to frontend
                if msg.event_type == "Notification" && !session_id.is_empty() {
                    let title = msg.title.clone().unwrap_or_else(|| "Notchai".to_string());
                    let message = msg.message.clone().unwrap_or_default();

                    // Trigger macOS native notification
                    let _ = app_handle
                        .notification()
                        .builder()
                        .title(&title)
                        .body(&message)
                        .show();

                    // Emit to frontend for collapsed view display
                    let notif_payload = NotificationPayload {
                        session_id: session_id.clone(),
                        title,
                        message,
                        timestamp: timestamp.clone(),
                    };
                    let _ = app_handle.emit("hook:notification", &notif_payload);
                }

                // Publish NormalizedEvent to the EventBus
                if !session_id.is_empty() {
                    if let Some(normalized) = map_hook_to_normalized_event(
                        &msg.event_type,
                        &session_id,
                        &timestamp,
                        &msg.tool_name,
                        &msg.tool_input,
                        &msg.agent,
                    ) {
                        bus.publish(normalized);
                    }
                }
            }
        });
    }
}

/// Determine AgentType from the agent field in a HookMessage.
fn agent_type_from_string(agent: &Option<String>) -> AgentType {
    match agent.as_deref() {
        Some("codex") => AgentType::Codex,
        Some("gemini") => AgentType::Gemini,
        Some("cursor") => AgentType::Cursor,
        _ => AgentType::Claude,
    }
}

/// Determine EventSource from the agent field in a HookMessage.
fn event_source_from_agent(agent: &Option<String>) -> EventSource {
    match agent.as_deref() {
        Some("codex") => EventSource::Notify,
        _ => EventSource::Hook,
    }
}

/// Map a hook event_type to a NormalizedEvent.
/// Returns None for event types that don't have a meaningful mapping.
fn map_hook_to_normalized_event(
    event_type: &str,
    session_id: &str,
    timestamp: &str,
    tool_name: &Option<String>,
    tool_input: &Option<String>,
    agent: &Option<String>,
) -> Option<NormalizedEvent> {
    let agent_type = agent_type_from_string(agent);
    let source = event_source_from_agent(agent);
    let sid = session_id.to_string();
    let ts = timestamp.to_string();

    match event_type {
        "PreToolUse" => Some(NormalizedEvent::ToolStarted {
            agent_type,
            session_id: sid,
            timestamp: ts,
            source,
            tool_name: tool_name.clone().unwrap_or_default(),
            tool_input: tool_input.clone(),
        }),
        "PostToolUse" => Some(NormalizedEvent::ToolCompleted {
            agent_type,
            session_id: sid,
            timestamp: ts,
            source,
            tool_name: tool_name.clone().unwrap_or_default(),
            status: "ok".to_string(),
            duration_ms: None,
            result_preview: None,
        }),
        "Stop" | "SubagentStop" | "SessionEnd" => Some(NormalizedEvent::SessionEnded {
            agent_type,
            session_id: sid,
            timestamp: ts,
            source,
        }),
        "SessionStart" => Some(NormalizedEvent::SessionStarted {
            agent_type,
            session_id: sid,
            timestamp: ts,
            source,
        }),
        "UserPromptSubmit" => Some(NormalizedEvent::StatusChanged {
            agent_type,
            session_id: sid,
            timestamp: ts,
            source,
            new_status: AgentStatus::Operating,
        }),
        "task_complete" => Some(NormalizedEvent::TaskCompleted {
            agent_type,
            session_id: sid,
            timestamp: ts,
            source,
        }),
        "Notification" => Some(NormalizedEvent::StatusChanged {
            agent_type,
            session_id: sid,
            timestamp: ts,
            source,
            new_status: AgentStatus::Operating,
        }),
        _ => {
            eprintln!(
                "[hook_server] unknown hook event type '{}' for session {}, publishing generic StatusChanged",
                event_type, session_id
            );
            Some(NormalizedEvent::StatusChanged {
                agent_type,
                session_id: sid,
                timestamp: ts,
                source,
                new_status: AgentStatus::Operating,
            })
        }
    }
}
