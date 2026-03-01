use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use serde_json;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{oneshot, Mutex};

use crate::hook_models::{
    HookMessage, HookStatusPayload, PermissionDecision, PermissionRequestPayload,
};

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
}

impl HookServer {
    fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            tool_use_id_cache: Arc::new(Mutex::new(HashMap::new())),
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
pub async fn start(app: AppHandle) {
    // Initialize the global server instance
    let server = HookServer::new();
    let pending = server.pending.clone();
    let tool_use_id_cache = server.tool_use_id_cache.clone();
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
                {
                    let cache_lock = cache.lock().await;
                    let _cached_tool_use_id = cache_lock
                        .get(&(session_id.clone(), tool_name.clone()))
                        .cloned();
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
                };

                let _ = app_handle.emit("hook:permission-request", &payload);

                // Create a oneshot channel and store the sender
                let (tx, rx) = oneshot::channel::<PermissionDecision>();
                {
                    let mut pending_lock = pending.lock().await;
                    pending_lock.insert(request_id.clone(), tx);
                }

                // Wait for the response from the UI
                match rx.await {
                    Ok(decision) => {
                        let mut response = serde_json::json!({
                            "decision": decision.decision,
                            "reason": decision.reason
                        });
                        if let Some(ref updated_input) = decision.updated_input {
                            response["updated_input"] = serde_json::Value::String(updated_input.clone());
                        }
                        let response_str = response.to_string() + "\n";
                        if let Err(e) = writer.write_all(response_str.as_bytes()).await {
                            eprintln!("[hook_server] write response error: {}", e);
                        }
                    }
                    Err(_) => {
                        // Sender was dropped (e.g., server shutting down), fail-open
                        eprintln!("[hook_server] permission request {} cancelled", request_id);
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

                // Emit status update for non-permission events
                if !session_id.is_empty() {
                    let payload = HookStatusPayload {
                        event_type: msg.event_type.clone(),
                        session_id,
                        cwd: msg.cwd.clone(),
                        tool_name: msg.tool_name.clone(),
                        agent: msg.agent.clone(),
                        timestamp,
                    };
                    let _ = app_handle.emit("hook:status-update", &payload);
                }
            }
        });
    }
}
