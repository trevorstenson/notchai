mod adapter;
mod adapters;
mod event_bus;
mod hook_installer;
mod hook_models;
mod hook_server;
mod models;
mod monitor;
mod notch;
mod otel_server;
mod process;
mod scanner;
mod transcript;
mod util;

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::atomic::AtomicU64;
use std::process::Command;
use std::path::PathBuf;
use std::collections::HashSet;
#[cfg(target_os = "macos")]
use std::sync::OnceLock;
#[cfg(target_os = "macos")]
use std::{thread, time::Duration};

use models::{AgentSession, NotchInfo, ScreenInfo, ToolCallInfo};
use monitor::AgentMonitor;
use serde::Serialize;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::ShortcutState;
use crate::hook_models::PermissionDecision;

#[cfg(target_os = "macos")]
const GENERIC_TERMINAL_APPS: &[&str] = &[
    "WezTerm",
    "Ghostty",
    "Alacritty",
    "kitty",
    "Hyper",
    "Tabby",
];

struct AppState {
    monitor: Mutex<AgentMonitor>,
    /// Current hover monitor stop flag. Protected by a Mutex so we can swap it
    /// when repositioning.
    hover_stop_flag: Mutex<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsPayload {
    hooks_enabled: bool,
    codex_hooks_enabled: bool,
    selected_screen: Option<usize>,
    sound_enabled: bool,
    auto_expand_on_approval: bool,
}

#[tauri::command]
fn get_sessions(state: tauri::State<'_, AppState>) -> Vec<AgentSession> {
    state.monitor.lock().unwrap().get_sessions()
}

#[tauri::command]
fn get_notch_info() -> NotchInfo {
    notch::detect_notch().info
}

#[tauri::command]
fn set_window_mouse_passthrough(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("main window not found")?;
    set_window_mouse_passthrough_for_window(&window, enabled)
}

#[tauri::command]
fn open_session_location(path: String) -> Result<(), String> {
    let normalized_path: PathBuf = {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err("Session has no folder path".to_string());
        }
        PathBuf::from(trimmed)
    };

    if !normalized_path.exists() {
        return Err(format!(
            "Session folder does not exist: {}",
            normalized_path.display()
        ));
    }

    let path_for_open = normalized_path.to_string_lossy().to_string();

    #[cfg(target_os = "macos")]
    {
        // If this session is currently running, prefer focusing the terminal app
        // that owns that specific Claude process instead of opening a new tab.
        if let Some(running_terminal_app) = detect_running_terminal_for_path(&path_for_open) {
            if focus_terminal_app(&running_terminal_app).is_ok() {
                return Ok(());
            }
        }

        // If a terminal app is already running, focus it only (no new tab/window).
        if app_is_running("Terminal") {
            if focus_terminal_app("Terminal").is_ok() {
                return Ok(());
            }
        }
        if app_is_running("iTerm2") {
            if focus_terminal_app("iTerm2").is_ok() {
                return Ok(());
            }
        }
        if app_is_running("iTerm") {
            if focus_terminal_app("iTerm").is_ok() {
                return Ok(());
            }
        }
        if app_is_running("Warp") && focus_warp_app().is_ok() {
            return Ok(());
        }
        for app_name in GENERIC_TERMINAL_APPS {
            if app_is_running(app_name) && focus_terminal_app(app_name).is_ok() {
                return Ok(());
            }
        }

        // No known terminal app is running: open a terminal at the target path.
        if open_in_iterm_app("iTerm2", &path_for_open).is_ok()
            || open_in_iterm_app("iTerm", &path_for_open).is_ok()
            || open_in_terminal(&path_for_open).is_ok()
        {
            return Ok(());
        }

        let status = Command::new("open")
            .arg(&path_for_open)
            .status()
            .map_err(|e| format!("failed to open project path: {}", e))?;
        if status.success() {
            Ok(())
        } else {
            Err("open command failed".to_string())
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let status = Command::new("xdg-open")
            .arg(&path_for_open)
            .status()
            .map_err(|e| format!("failed to open project path: {}", e))?;
        if status.success() {
            Ok(())
        } else {
            Err("open command failed".to_string())
        }
    }
}

#[tauri::command]
fn resume_session(session_id: String, path: String) -> Result<(), String> {
    let trimmed_id = session_id.trim();
    if trimmed_id.is_empty() {
        return Err("Missing session id".to_string());
    }

    let normalized_path: PathBuf = {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err("Session has no folder path".to_string());
        }
        PathBuf::from(trimmed)
    };

    if !normalized_path.exists() {
        return Err(format!(
            "Session folder does not exist: {}",
            normalized_path.display()
        ));
    }

    let path_for_open = normalized_path.to_string_lossy().to_string();

    #[cfg(target_os = "macos")]
    {
        // Safety: if session is actually still running, focus existing terminal app
        // instead of spawning resume tabs/windows.
        if let Some(running_terminal_app) = detect_running_terminal_for_path(&path_for_open) {
            if focus_terminal_app(&running_terminal_app).is_ok() {
                return Ok(());
            }
        }

        if app_is_running("Terminal") && resume_in_terminal(trimmed_id, &path_for_open).is_ok() {
            return Ok(());
        }
        if app_is_running("iTerm2") && resume_in_iterm_app("iTerm2", trimmed_id, &path_for_open).is_ok() {
            return Ok(());
        }
        if app_is_running("iTerm") && resume_in_iterm_app("iTerm", trimmed_id, &path_for_open).is_ok() {
            return Ok(());
        }
        if app_is_running("Warp") && resume_in_warp(trimmed_id, &path_for_open).is_ok() {
            return Ok(());
        }
        for app_name in GENERIC_TERMINAL_APPS {
            if app_is_running(app_name)
                && resume_in_generic_terminal_app(app_name, trimmed_id, &path_for_open).is_ok()
            {
                return Ok(());
            }
        }

        if resume_in_terminal(trimmed_id, &path_for_open).is_ok()
            || resume_in_iterm_app("iTerm2", trimmed_id, &path_for_open).is_ok()
            || resume_in_iterm_app("iTerm", trimmed_id, &path_for_open).is_ok()
            || resume_in_warp(trimmed_id, &path_for_open).is_ok()
        {
            return Ok(());
        }

        Err("Could not start resume command in a terminal".to_string())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("resume_session is currently only implemented on macOS".to_string())
    }
}

#[tauri::command]
async fn respond_to_approval(
    request_id: String,
    decision: String,
    reason: Option<String>,
    updated_input: Option<String>,
    updated_permissions: Option<String>,
) -> Result<(), String> {
    let server = hook_server::get_server().ok_or("Hook server not running")?;
    server
        .respond(
            &request_id,
            PermissionDecision {
                decision,
                reason,
                updated_input,
                updated_permissions,
            },
        )
        .await
}

/// Resolve the bundled notchai-hook.py, trying Tauri resource paths and a dev-mode fallback.
fn resolve_hook_script(app: &tauri::AppHandle) -> Option<PathBuf> {
    // Try Tauri resource resolution (works in production bundles)
    for name in &["resources/notchai-hook.py", "notchai-hook.py"] {
        if let Ok(p) = app.path().resolve(*name, tauri::path::BaseDirectory::Resource) {
            if p.exists() {
                return Some(p);
            }
        }
    }
    // Dev-mode fallback: file lives at <project-root>/resources/notchai-hook.py
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|root| root.join("resources").join("notchai-hook.py"))?;
    if dev_path.exists() {
        return Some(dev_path);
    }
    None
}

/// Resolve the bundled notchai-codex-notify.sh, trying Tauri resource paths and a dev-mode fallback.
fn resolve_codex_notify_script(app: &tauri::AppHandle) -> Option<PathBuf> {
    for name in &[
        "resources/notchai-codex-notify.sh",
        "notchai-codex-notify.sh",
    ] {
        if let Ok(p) = app.path().resolve(*name, tauri::path::BaseDirectory::Resource) {
            if p.exists() {
                return Some(p);
            }
        }
    }
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|root| root.join("resources").join("notchai-codex-notify.sh"))?;
    if dev_path.exists() {
        return Some(dev_path);
    }
    None
}

#[tauri::command]
fn toggle_hooks_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        let resource_path = resolve_hook_script(&app)
            .ok_or("Cannot find notchai-hook.py in app bundle or project resources")?;
        hook_installer::install_hooks(&resource_path)?;
    } else {
        hook_installer::uninstall_hooks()?;
    }
    hook_installer::set_hooks_enabled(enabled)
}

#[tauri::command]
fn get_hooks_enabled() -> bool {
    hook_installer::get_hooks_enabled()
}

#[tauri::command]
fn toggle_codex_hooks_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        let resource_path = resolve_codex_notify_script(&app)
            .ok_or("Cannot find notchai-codex-notify.sh in app bundle or project resources")?;
        hook_installer::install_codex_hooks(&resource_path)?;
    } else {
        hook_installer::uninstall_codex_hooks()?;
    }
    hook_installer::set_codex_hooks_enabled(enabled)
}

#[tauri::command]
fn get_codex_hooks_enabled() -> bool {
    hook_installer::get_codex_hooks_enabled()
}

#[tauri::command]
fn get_session_tool_calls(
    session_id: String,
    state: tauri::State<'_, AppState>,
) -> Vec<ToolCallInfo> {
    state.monitor.lock().unwrap().get_tool_calls(&session_id)
}

#[tauri::command]
fn play_sound(name: String) {
    #[cfg(target_os = "macos")]
    {
        use objc::runtime::{Class, Object, BOOL};
        use objc::{msg_send, sel, sel_impl};

        unsafe {
            let ns_sound_class = match Class::get("NSSound") {
                Some(c) => c,
                None => return,
            };

            // Convert Rust string to NSString
            let ns_string_class = match Class::get("NSString") {
                Some(c) => c,
                None => return,
            };
            let c_name = std::ffi::CString::new(name.as_str()).unwrap_or_default();
            let ns_name: *mut Object = msg_send![ns_string_class,
                stringWithUTF8String: c_name.as_ptr()];
            if ns_name.is_null() {
                return;
            }

            let sound: *mut Object = msg_send![ns_sound_class, soundNamed: ns_name];
            if sound.is_null() {
                return;
            }

            let _: BOOL = msg_send![sound, play];
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = name;
    }
}

#[tauri::command]
fn play_haptic() {
    #[cfg(target_os = "macos")]
    {
        use objc::runtime::{Class, Object};
        use objc::{msg_send, sel, sel_impl};

        unsafe {
            let manager_class = match Class::get("NSHapticFeedbackManager") {
                Some(c) => c,
                None => return,
            };

            let performer: *mut Object = msg_send![manager_class, defaultPerformer];
            if performer.is_null() {
                return;
            }

            // NSHapticFeedbackPattern.Generic = 0, NSHapticFeedbackPerformanceTime.Default = 0
            let pattern: usize = 0;
            let performance_time: usize = 0;
            let _: () = msg_send![performer,
                performFeedbackPattern: pattern
                performanceTime: performance_time];
        }
    }
}

#[tauri::command]
fn get_sound_enabled() -> bool {
    hook_installer::get_sound_enabled()
}

#[tauri::command]
fn list_screens() -> Vec<ScreenInfo> {
    notch::list_screens()
}

/// Return a usable selected screen index.
/// Resolves by saved screen name first (indexes can shift).
/// Returns None (auto-detect) if the saved monitor is no longer connected.
fn get_valid_selected_screen() -> Option<usize> {
    let saved_index = hook_installer::get_selected_screen();
    let saved_name = hook_installer::get_selected_screen_name();
    let resolved_index = notch::resolve_screen_index(saved_index, saved_name.as_deref());

    if resolved_index != saved_index {
        if let Err(e) = hook_installer::set_selected_screen(resolved_index, saved_name.as_deref()) {
            eprintln!(
                "[settings] failed to persist resolved selected_screen {:?} (was {:?}): {}",
                resolved_index, saved_index, e
            );
        }
    }

    resolved_index
}

/// Resolve the screen index used for window placement.
/// Explicit user selection wins; otherwise prefer primary display (index 0).
fn effective_screen_index_for_placement(selected_screen: Option<usize>) -> Option<usize> {
    selected_screen.or_else(|| notch::list_screens().first().map(|screen| screen.index))
}

#[tauri::command]
fn get_selected_screen() -> Option<usize> {
    get_valid_selected_screen()
}

#[tauri::command]
fn set_selected_screen(
    index: Option<usize>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let name = index.and_then(|i| {
        notch::list_screens()
            .into_iter()
            .find(|s| s.index == i)
            .map(|s| s.name)
    });
    hook_installer::set_selected_screen(index, name.as_deref())?;
    reposition_window(index, &app);
    Ok(())
}

#[tauri::command]
fn get_settings() -> SettingsPayload {
    SettingsPayload {
        hooks_enabled: hook_installer::get_hooks_enabled(),
        codex_hooks_enabled: hook_installer::get_codex_hooks_enabled(),
        selected_screen: get_valid_selected_screen(),
        sound_enabled: hook_installer::get_sound_enabled(),
        auto_expand_on_approval: hook_installer::get_auto_expand_on_approval(),
    }
}

#[tauri::command]
fn save_settings(
    hooks_enabled: bool,
    codex_hooks_enabled: bool,
    selected_screen: Option<usize>,
    sound_enabled: bool,
    auto_expand_on_approval: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Handle hooks toggle
    let current_hooks = hook_installer::get_hooks_enabled();
    if hooks_enabled != current_hooks {
        if hooks_enabled {
            let resource_path = resolve_hook_script(&app)
                .ok_or("Cannot find notchai-hook.py in app bundle or project resources")?;
            hook_installer::install_hooks(&resource_path)?;
        } else {
            hook_installer::uninstall_hooks()?;
        }
        hook_installer::set_hooks_enabled(hooks_enabled)?;
    }

    // Handle Codex hooks toggle
    let current_codex_hooks = hook_installer::get_codex_hooks_enabled();
    if codex_hooks_enabled != current_codex_hooks {
        if codex_hooks_enabled {
            let resource_path = resolve_codex_notify_script(&app)
                .ok_or("Cannot find notchai-codex-notify.sh in app bundle or project resources")?;
            hook_installer::install_codex_hooks(&resource_path)?;
        } else {
            hook_installer::uninstall_codex_hooks()?;
        }
        hook_installer::set_codex_hooks_enabled(codex_hooks_enabled)?;
    }

    // Handle screen selection
    let current_screen = get_valid_selected_screen();
    if selected_screen != current_screen {
        let name = selected_screen.and_then(|i| {
            notch::list_screens()
                .into_iter()
                .find(|s| s.index == i)
                .map(|s| s.name)
        });
        hook_installer::set_selected_screen(selected_screen, name.as_deref())?;
        reposition_window(selected_screen, &app);
    }

    // Handle sound toggle
    hook_installer::set_sound_enabled(sound_enabled)?;

    // Handle auto-expand on approval toggle
    hook_installer::set_auto_expand_on_approval(auto_expand_on_approval)?;

    Ok(())
}

/// Reposition the window on the selected screen and restart the hover monitor.
fn reposition_window(
    screen_index: Option<usize>,
    #[allow(unused_variables)] app: &tauri::AppHandle,
) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let effective_screen = effective_screen_index_for_placement(screen_index);
    let detection = notch::detect_notch_on_screen(effective_screen);
    let notch = &detection.info;
    let hover_width = (notch.width + 340.0).max(540.0);
    let hover_height = 420.0;
    let x = notch.center_x() - hover_width / 2.0;

    window
        .set_position(tauri::LogicalPosition::new(x, notch.y))
        .ok();
    window
        .set_size(tauri::LogicalSize::new(hover_width, hover_height))
        .ok();

    #[cfg(target_os = "macos")]
    {
        let state = app.state::<AppState>();
        // Stop the old hover monitor thread and swap in a new flag
        let new_flag = Arc::new(AtomicBool::new(false));
        {
            let mut flag_guard = state.hover_stop_flag.lock().unwrap();
            flag_guard.store(true, Ordering::SeqCst);
            *flag_guard = new_flag.clone();
        }

        start_global_hover_monitor_with_flag(
            app.clone(),
            notch.center_x(),
            hover_width,
            24.0,
            420.0,
            detection.screen_top_macos_y,
            new_flag,
        );
    }
}

#[cfg(target_os = "macos")]
fn set_window_mouse_passthrough_for_window(
    window: &tauri::WebviewWindow,
    enabled: bool,
) -> Result<(), String> {
    use objc::runtime::{Object, NO, YES};
    use objc::{msg_send, sel, sel_impl};

    let ns_win = window
        .ns_window()
        .map_err(|e| format!("failed to access NSWindow: {}", e))?;
    let ns_win = ns_win as *mut Object;

    unsafe {
        let flag = if enabled { YES } else { NO };
        let _: () = msg_send![ns_win, setIgnoresMouseEvents: flag];
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn set_window_mouse_passthrough_for_window(
    _window: &tauri::WebviewWindow,
    _enabled: bool,
) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn app_is_running(app_name: &str) -> bool {
    Command::new("pgrep")
        .args(["-x", app_name])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn focus_warp_app() -> Result<(), String> {
    // `open -a` focuses the app if running and avoids creating a new tab.
    let status = Command::new("open")
        .args(["-a", "Warp"])
        .status()
        .map_err(|e| format!("warp focus failed: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err("warp focus returned non-zero".to_string())
    }
}

#[cfg(target_os = "macos")]
fn focus_terminal_app(app_name: &str) -> Result<(), String> {
    if app_name == "Warp" {
        focus_warp_app()
    } else if app_name == "iTerm" || app_name == "iTerm2" {
        // Different installs can expose either app name.
        focus_app("iTerm").or_else(|_| focus_app("iTerm2"))
    } else {
        focus_app(app_name)
    }
}

#[cfg(target_os = "macos")]
fn focus_app(app_name: &str) -> Result<(), String> {
    let status = Command::new("open")
        .args(["-a", app_name])
        .status()
        .map_err(|e| format!("{} focus failed: {}", app_name, e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} focus returned non-zero", app_name))
    }
}

#[cfg(target_os = "macos")]
fn detect_running_terminal_for_path(target_path: &str) -> Option<String> {
    let pids = collect_claude_runtime_pids();
    if pids.is_empty() {
        return None;
    }

    let normalized_target = normalize_path_for_match(target_path);
    let mut best_match: Option<(i32, String)> = None;

    for pid in pids {
        let Some(cwd) = pid_cwd(pid) else {
            continue;
        };
        let score = path_match_score(&cwd, &normalized_target);
        if score <= 0 {
            continue;
        }
        if let Some(app_name) = terminal_app_for_pid(pid) {
            let replace = best_match
                .as_ref()
                .map_or(true, |(best_score, _)| score > *best_score);
            if replace {
                best_match = Some((score, app_name.to_string()));
            }
        }
    }
    best_match.map(|(_, app_name)| app_name)
}

#[cfg(target_os = "macos")]
fn collect_claude_runtime_pids() -> Vec<u32> {
    let mut pids: HashSet<u32> = HashSet::new();

    // Primary source: exact executable name catches plain `claude`.
    if let Ok(output) = Command::new("pgrep").args(["-x", "claude"]).output() {
        if output.status.success() {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    pids.insert(pid);
                }
            }
        }
    }

    // Secondary source: snapshot catches wrapper invocations with args.
    let snapshot = process::ProcessSnapshot::capture();
    for pid in snapshot.get_matching_pids(|line| {
        (line.contains("/claude") || line.contains("claude ")) && !line.contains("grep")
    }) {
        pids.insert(pid);
    }

    let mut merged: Vec<u32> = pids.into_iter().collect();
    merged.sort_unstable();
    merged
}

#[cfg(target_os = "macos")]
fn normalize_path_for_match(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    std::fs::canonicalize(trimmed)
        .unwrap_or_else(|_| PathBuf::from(trimmed))
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string()
}

#[cfg(target_os = "macos")]
fn path_depth(path: &str) -> i32 {
    path.split('/').filter(|segment| !segment.is_empty()).count() as i32
}

#[cfg(target_os = "macos")]
fn path_match_score(process_cwd: &str, session_path: &str) -> i32 {
    let cwd = normalize_path_for_match(process_cwd);
    let session = normalize_path_for_match(session_path);
    if cwd.is_empty() || session.is_empty() {
        return 0;
    }
    if cwd == session {
        // Strongest match: same exact folder.
        return 3000 + path_depth(&cwd);
    }
    if cwd.starts_with(&format!("{}/", session)) {
        // Process is in a subfolder under the session path.
        return 2000 + path_depth(&cwd) - path_depth(&session);
    }
    if session.starts_with(&format!("{}/", cwd)) {
        // Session path is deeper than process cwd (weaker, still plausible).
        return 1000 + path_depth(&cwd);
    }
    0
}

#[cfg(target_os = "macos")]
fn pid_cwd(pid: u32) -> Option<String> {
    let output = Command::new("lsof")
        .args(["-a", "-d", "cwd", "-p", &pid.to_string(), "-Fn"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix('n') {
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn ps_ppid(pid: u32) -> Option<u32> {
    let output = Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse::<u32>().ok()
}

#[cfg(target_os = "macos")]
fn ps_args(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let args = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if args.is_empty() {
        None
    } else {
        Some(args)
    }
}

#[cfg(target_os = "macos")]
fn terminal_app_from_args(args: &str) -> Option<&'static str> {
    let lower = args.to_lowercase();
    if lower.contains("warp.app/contents/macos") || lower.contains("terminal-server --parent-pid") {
        Some("Warp")
    } else if lower.contains("terminal.app/contents/macos/terminal") {
        Some("Terminal")
    } else if lower.contains("iterm2.app/contents/macos/iterm2")
        || lower.contains("iterm.app/contents/macos/iterm2")
        || lower.contains("/library/application support/iterm2/itermserver-")
    {
        Some("iTerm")
    } else if lower.contains("wezterm.app/contents/macos") || lower.contains("wezterm-gui") {
        Some("WezTerm")
    } else if lower.contains("ghostty.app/contents/macos") || lower.contains("ghostty") {
        Some("Ghostty")
    } else if lower.contains("alacritty.app/contents/macos") || lower.contains("alacritty") {
        Some("Alacritty")
    } else if lower.contains("/kitty.app/contents/macos") || lower.contains("/kitty ") {
        Some("kitty")
    } else if lower.contains("hyper.app/contents/macos") || lower.contains("/hyper ") {
        Some("Hyper")
    } else if lower.contains("tabby.app/contents/macos") || lower.contains("/tabby ") {
        Some("Tabby")
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn terminal_app_for_pid(pid: u32) -> Option<&'static str> {
    let mut current = pid;
    for _ in 0..14 {
        let args = ps_args(current)?;
        if let Some(app_name) = terminal_app_from_args(&args) {
            return Some(app_name);
        }
        let ppid = ps_ppid(current)?;
        if ppid <= 1 || ppid == current {
            break;
        }
        current = ppid;
    }
    None
}

#[cfg(target_os = "macos")]
fn open_in_terminal(project_path: &str) -> Result<(), String> {
    let script = r#"
on run argv
  set p to item 1 of argv
  tell application "Terminal"
    activate
    do script ("cd " & quoted form of p)
  end tell
end run
"#;
    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg("--")
        .arg(project_path)
        .status()
        .map_err(|e| format!("terminal osascript failed: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err("terminal osascript returned non-zero".to_string())
    }
}

#[cfg(target_os = "macos")]
fn open_in_iterm_app(app_name: &str, project_path: &str) -> Result<(), String> {
    let script = r#"
on run argv
  set appName to item 1 of argv
  set p to item 2 of argv
  tell application appName
    activate
    if (count of windows) = 0 then
      create window with default profile
    end if
    tell current window
      create tab with default profile command ("cd " & quoted form of p)
    end tell
  end tell
end run
"#;
    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg("--")
        .arg(app_name)
        .arg(project_path)
        .status()
        .map_err(|e| format!("iterm osascript failed: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err("iterm osascript returned non-zero".to_string())
    }
}

#[cfg(target_os = "macos")]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(target_os = "macos")]
fn open_command_for_path(project_path: &str) -> String {
    format!("cd {}", shell_quote(project_path))
}

#[cfg(target_os = "macos")]
fn resume_command(session_id: &str, project_path: &str) -> String {
    format!(
        "cd {} && claude --resume {}",
        shell_quote(project_path),
        shell_quote(session_id)
    )
}

#[cfg(target_os = "macos")]
fn run_command_in_generic_terminal_app(app_name: &str, command: &str) -> Result<(), String> {
    // Best-effort generic flow for terminals that accept Cmd+N and text input.
    focus_app(app_name)?;
    let script = r#"
on run argv
  set appName to item 1 of argv
  set cmd to item 2 of argv
  tell application appName
    activate
  end tell
  delay 0.12
  tell application "System Events"
    keystroke "n" using command down
    delay 0.08
    set the clipboard to cmd
    keystroke "v" using command down
    delay 0.05
    keystroke return
  end tell
end run
"#;
    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg("--")
        .arg(app_name)
        .arg(command)
        .status()
        .map_err(|e| format!("{} generic command failed: {}", app_name, e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} generic command returned non-zero", app_name))
    }
}

#[cfg(target_os = "macos")]
fn resume_in_generic_terminal_app(
    app_name: &str,
    session_id: &str,
    project_path: &str,
) -> Result<(), String> {
    run_command_in_generic_terminal_app(app_name, &resume_command(session_id, project_path))
}

#[cfg(target_os = "macos")]
fn resume_in_iterm_app(app_name: &str, session_id: &str, project_path: &str) -> Result<(), String> {
    let command = resume_command(session_id, project_path);
    let script = r#"
on run argv
  set appName to item 1 of argv
  set cmd to item 2 of argv
  tell application appName
    activate
    if (count of windows) = 0 then
      create window with default profile
    end if
    tell current window
      create tab with default profile command cmd
    end tell
  end tell
end run
"#;
    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg("--")
        .arg(app_name)
        .arg(command)
        .status()
        .map_err(|e| format!("iterm resume failed: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err("iterm resume returned non-zero".to_string())
    }
}

#[cfg(target_os = "macos")]
fn resume_in_terminal(session_id: &str, project_path: &str) -> Result<(), String> {
    let command = resume_command(session_id, project_path);
    let script = r#"
on run argv
  set cmd to item 1 of argv
  tell application "Terminal"
    activate
    do script cmd
  end tell
end run
"#;
    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg("--")
        .arg(command)
        .status()
        .map_err(|e| format!("terminal resume failed: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err("terminal resume returned non-zero".to_string())
    }
}

#[cfg(target_os = "macos")]
fn resume_in_warp(session_id: &str, project_path: &str) -> Result<(), String> {
    let encoded_path = project_path.replace(' ', "%20");
    let uri = format!("warp://action/new_tab?path={}", encoded_path);
    let status = Command::new("open")
        .arg(uri)
        .status()
        .map_err(|e| format!("warp new tab failed: {}", e))?;
    if !status.success() {
        return Err("warp new tab returned non-zero".to_string());
    }

    // Best-effort: focus Warp, paste command, then send Enter.
    let command = format!("claude --resume {}", session_id);
    let script = r#"
on run argv
  set cmd to item 1 of argv
  tell application "Warp"
    activate
  end tell
  delay 0.35
  set the clipboard to cmd
  tell application "System Events"
    keystroke "v" using command down
    delay 0.08
    keystroke return
    delay 0.08
    key code 36
    delay 0.05
    key code 76
  end tell
end run
"#;
    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg("--")
        .arg(command)
        .status()
        .map_err(|e| format!("warp resume typing failed: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err("warp resume typing returned non-zero".to_string())
    }
}

#[cfg(target_os = "macos")]
fn start_global_hover_monitor(
    app: tauri::AppHandle,
    center_x: f64,
    hover_width: f64,
    hover_height: f64,
    expanded_height: f64,
    screen_top_macos_y: f64,
    stop_flag: Arc<AtomicBool>,
) {
    start_global_hover_monitor_with_flag(
        app,
        center_x,
        hover_width,
        hover_height,
        expanded_height,
        screen_top_macos_y,
        stop_flag,
    );
}

#[cfg(target_os = "macos")]
fn start_global_hover_monitor_with_flag(
    app: tauri::AppHandle,
    center_x: f64,
    hover_width: f64,
    hover_height: f64,
    expanded_height: f64,
    screen_top_macos_y: f64,
    stop_flag: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut was_inside = false;

        loop {
            if stop_flag.load(Ordering::SeqCst) {
                return;
            }

            if let Some((mouse_x, mouse_y_from_top)) =
                current_mouse_position_from_top_left(screen_top_macos_y)
            {
                let left = center_x - hover_width / 2.0;
                let right = left + hover_width;
                let active_height = if was_inside {
                    expanded_height
                } else {
                    hover_height
                };
                let inside = mouse_x >= left
                    && mouse_x <= right
                    && mouse_y_from_top >= 0.0
                    && mouse_y_from_top <= active_height;

                if inside != was_inside {
                    if inside {
                        // Activate the app so macOS delivers events to our
                        // webview immediately, even when another app has focus.
                        unsafe {
                            use objc::runtime::{Object, YES};
                            use objc::{class, msg_send, sel, sel_impl};
                            let ns_app: *mut Object =
                                msg_send![class!(NSApplication), sharedApplication];
                            let _: () =
                                msg_send![ns_app, activateIgnoringOtherApps: YES];
                        }
                        let _ = app.emit("open-panel", ());
                    } else {
                        let _ = app.emit("close-panel", ());
                    }
                    was_inside = inside;
                }
            }

            thread::sleep(Duration::from_millis(8));
        }
    });
}

#[cfg(target_os = "macos")]
fn current_mouse_position_from_top_left(screen_top_macos_y: f64) -> Option<(f64, f64)> {
    use objc::runtime::Class;
    use objc::{msg_send, sel, sel_impl};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    unsafe {
        let ns_event_class = Class::get("NSEvent")?;
        let mouse: CGPoint = msg_send![ns_event_class, mouseLocation];
        // Convert macOS bottom-left coords to distance from top of the notch screen.
        let from_top = screen_top_macos_y - mouse.y;
        Some((mouse.x, from_top))
    }
}

/// Global AppHandle so the ObjC notification callback can access it.
#[cfg(target_os = "macos")]
static SCREEN_CHANGE_APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Debounce: skip display change events that arrive within 500ms of the last handled one.
#[cfg(target_os = "macos")]
static LAST_SCREEN_CHANGE_MS: AtomicU64 = AtomicU64::new(0);

/// Register an observer for NSApplicationDidChangeScreenParametersNotification.
/// When monitors are connected/disconnected/rearranged, macOS fires this notification
/// and we automatically reposition the window.
#[cfg(target_os = "macos")]
fn register_screen_change_observer(app_handle: tauri::AppHandle) {
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel};
    use objc::{msg_send, sel, sel_impl};

    SCREEN_CHANGE_APP_HANDLE.set(app_handle).ok();

    extern "C" fn screen_did_change(_this: &Object, _cmd: Sel, _notification: *mut Object) {
        if let Some(app) = SCREEN_CHANGE_APP_HANDLE.get() {
            handle_screen_configuration_change(app);
        }
    }

    unsafe {
        let superclass = Class::get("NSObject").expect("NSObject class not found");
        let mut decl = ClassDecl::new("NotchaiScreenObserver", superclass)
            .expect("Failed to declare NotchaiScreenObserver class");

        decl.add_method(
            sel!(screenDidChange:),
            screen_did_change as extern "C" fn(&Object, Sel, *mut Object),
        );

        let observer_class = decl.register();

        let observer: *mut Object = msg_send![observer_class, alloc];
        let observer: *mut Object = msg_send![observer, init];

        let ns_string_class = Class::get("NSString").expect("NSString class not found");
        let notif_name_cstr = std::ffi::CString::new(
            "NSApplicationDidChangeScreenParametersNotification"
        ).unwrap();
        let notif_name: *mut Object = msg_send![
            ns_string_class,
            stringWithUTF8String: notif_name_cstr.as_ptr()
        ];

        let nc_class = Class::get("NSNotificationCenter").expect("NSNotificationCenter not found");
        let center: *mut Object = msg_send![nc_class, defaultCenter];

        let _: () = msg_send![center,
            addObserver: observer
            selector: sel!(screenDidChange:)
            name: notif_name
            object: std::ptr::null::<Object>()
        ];

        // Observer must live for the app lifetime — intentionally leak it.
        std::mem::forget(observer);
    }
}

/// Called when macOS fires a display configuration change notification.
/// Re-resolves the saved screen by name and repositions the window.
#[cfg(target_os = "macos")]
fn handle_screen_configuration_change(app: &tauri::AppHandle) {
    // Debounce: macOS can fire multiple notifications in quick succession
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let last = LAST_SCREEN_CHANGE_MS.load(Ordering::SeqCst);
    if now_ms.saturating_sub(last) < 500 {
        return;
    }
    LAST_SCREEN_CHANGE_MS.store(now_ms, Ordering::SeqCst);

    let saved_index = hook_installer::get_selected_screen();
    let saved_name = hook_installer::get_selected_screen_name();
    let resolved_index = notch::resolve_screen_index(saved_index, saved_name.as_deref());

    // Persist the resolved index so get_settings() returns the correct value
    if resolved_index != saved_index {
        let _ = hook_installer::set_selected_screen(resolved_index, saved_name.as_deref());
    }

    reposition_window(resolved_index, app);

    // Tell the frontend so SettingsView can refresh its screen list
    let _ = app.emit("screens-changed", ());
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shortcut_plugin = tauri_plugin_global_shortcut::Builder::default()
        .with_shortcut("CommandOrControl+Shift+N")
        .expect("invalid shortcut")
        .with_handler(|app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = app.emit("open-panel", ());
            }
        })
        .build();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(shortcut_plugin)
        .manage(AppState {
            monitor: Mutex::new(AgentMonitor::new(vec![
                Box::new(adapters::claude::ClaudeAdapter::new()),
                Box::new(adapters::codex::CodexAdapter::new()),
                Box::new(adapters::cursor::CursorAdapter::new()),
                Box::new(adapters::gemini::GeminiAdapter::new()),
            ])),
            hover_stop_flag: Mutex::new(Arc::new(AtomicBool::new(false))),
        })
        .setup(|app| {
            // Install hooks if enabled and spawn the socket server
            let app_handle = app.handle().clone();
            if let Some(resource_path) = resolve_hook_script(app.handle()) {
                if let Err(e) = hook_installer::install_hooks_if_enabled(&resource_path) {
                    eprintln!("[hooks] Claude hooks install failed: {}", e);
                }
            } else {
                eprintln!("[hooks] could not resolve notchai-hook.py resource path");
            }

            // Install Codex notify hooks if enabled
            if let Some(codex_script_path) = resolve_codex_notify_script(app.handle()) {
                if let Err(e) = hook_installer::install_codex_hooks_if_enabled(&codex_script_path) {
                    eprintln!("[hooks] Codex hooks install failed: {}", e);
                }
            } else {
                eprintln!("[hooks] could not resolve notchai-codex-notify.sh resource path");
            }

            // Create the event bus for unified event pipeline
            let event_bus = event_bus::EventBus::new();

            // Spawn the hook socket server as a tokio task with EventBus
            let server_handle = app_handle.clone();
            let hook_bus = event_bus.clone();
            tauri::async_runtime::spawn(async move {
                hook_server::start(server_handle, hook_bus).await;
            });

            // Spawn the OTEL HTTP/protobuf ingestion server with EventBus
            let otel_bus = event_bus.clone();
            tauri::async_runtime::spawn(async move {
                otel_server::start(otel_bus).await;
            });

            // Spawn the EventBus → Tauri bridge: forwards NormalizedEvent to the frontend
            {
                let bridge_handle = app_handle.clone();
                let mut rx = event_bus.subscribe();
                tauri::async_runtime::spawn(async move {
                    loop {
                        match rx.recv().await {
                            Ok(event) => {
                                let _ = bridge_handle.emit("event-bus:normalized-event", &event);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                eprintln!("[event-bus] bridge lagged, skipped {} events", n);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                eprintln!("[event-bus] channel closed, bridge stopping");
                                break;
                            }
                        }
                    }
                });
            }

            let window = app.get_webview_window("main").unwrap();

            // Detect notch and position an invisible hover zone over it.
            // Respects saved screen selection; auto-detects if none is set.
            let selected_screen = get_valid_selected_screen();
            let effective_screen = effective_screen_index_for_placement(selected_screen);
            let detection = notch::detect_notch_on_screen(effective_screen);
            let notch = detection.info;
            let hover_width = (notch.width + 340.0).max(540.0);
            // Debug-first sizing: keep the window tall so expanded content is never clipped.
            let hover_height = 420.0;
            let x = notch.center_x() - hover_width / 2.0;

            window
                .set_position(tauri::LogicalPosition::new(x, notch.y))
                .ok();
            window
                .set_size(tauri::LogicalSize::new(hover_width, hover_height))
                .ok();

            #[cfg(target_os = "macos")]
            {
                // Global hover monitor so open/close works even when another app is focused.
                let stop_flag = app.state::<AppState>().hover_stop_flag.lock().unwrap().clone();
                start_global_hover_monitor(
                    app.handle().clone(),
                    notch.center_x(),
                    hover_width,
                    24.0,
                    420.0,
                    detection.screen_top_macos_y,
                    stop_flag,
                );
            }

            // Force truly transparent window on macOS and place it above the menu bar
            // so the hover zone can receive mouse events (otherwise the menu bar is on top).
            #[cfg(target_os = "macos")]
            {
                use objc::runtime::{Object, NO, YES};
                use objc::{class, msg_send, sel, sel_impl};

                // Accessory policy: no menu bar or dock icon, even when activated
                unsafe {
                    let ns_app: *mut Object =
                        msg_send![class!(NSApplication), sharedApplication];
                    let _: () = msg_send![ns_app, setActivationPolicy: 1i64];
                }

                if let Ok(ns_win) = window.ns_window() {
                    let ns_win = ns_win as *mut Object;
                    unsafe {
                        let clear: *mut Object = msg_send![class!(NSColor), clearColor];
                        let _: () = msg_send![ns_win, setBackgroundColor: clear];
                        let _: () = msg_send![ns_win, setOpaque: NO];
                        let _: () = msg_send![ns_win, setHasShadow: NO];
                        // NSStatusWindowLevel = 25 — above menu bar so we get hover near the notch
                        let level: i64 = 25;
                        let _: () = msg_send![ns_win, setLevel: level];
                        // Required for mouse enter/leave and hover; default is NO
                        let _: () = msg_send![ns_win, setAcceptsMouseMovedEvents: YES];
                    }
                }

                set_window_mouse_passthrough_for_window(&window, true).ok();
            }

            // Make visible on all workspaces
            window.set_visible_on_all_workspaces(true).ok();

            // Listen for monitor connect/disconnect events to auto-reposition
            #[cfg(target_os = "macos")]
            register_screen_change_observer(app.handle().clone());

            window.show().ok();

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_sessions,
            get_notch_info,
            set_window_mouse_passthrough,
            open_session_location,
            resume_session,
            respond_to_approval,
            toggle_hooks_enabled,
            get_hooks_enabled,
            toggle_codex_hooks_enabled,
            get_codex_hooks_enabled,
            get_session_tool_calls,
            play_sound,
            play_haptic,
            get_sound_enabled,
            list_screens,
            get_selected_screen,
            set_selected_screen,
            get_settings,
            save_settings
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                // Clean up the Unix socket on app exit
                let _ = std::fs::remove_file("/tmp/notchai.sock");
            }
        });
}
