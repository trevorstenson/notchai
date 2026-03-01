mod adapter;
mod adapters;
mod hook_installer;
mod hook_models;
mod hook_server;
mod models;
mod monitor;
mod notch;
mod process;
mod scanner;
mod transcript;
mod util;

use std::sync::Mutex;
use std::process::Command;
use std::path::PathBuf;
use std::collections::HashSet;
#[cfg(target_os = "macos")]
use std::{thread, time::Duration};

use models::{AgentSession, NotchInfo};
use monitor::AgentMonitor;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::ShortcutState;
use crate::hook_models::PermissionDecision;
use crate::process::ProcessDetector;

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
) -> Result<(), String> {
    let server = hook_server::get_server().ok_or("Hook server not running")?;
    server
        .respond(
            &request_id,
            PermissionDecision {
                decision,
                reason,
            },
        )
        .await
}

#[tauri::command]
fn toggle_hooks_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        // Resolve source script from the known install destination
        // (the script is already installed at ~/.claude/hooks/notchai-hook.py,
        //  but we install from the resources dir — use the bundled copy)
        let source = dirs::home_dir()
            .map(|h| h.join(".claude").join("hooks").join("notchai-hook.py"))
            .ok_or("Cannot determine home directory")?;
        // If the script already exists, use it as source (reinstall in-place)
        // Otherwise, this is a fresh enable — try the resources fallback
        if source.exists() {
            hook_installer::install_hooks(&source)?;
        }
    } else {
        hook_installer::uninstall_hooks()?;
    }
    hook_installer::set_hooks_enabled(enabled)
}

#[tauri::command]
fn get_hooks_enabled() -> bool {
    hook_installer::get_hooks_enabled()
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

    // Secondary source: existing scanner catches wrapper invocations with args.
    let detector = ProcessDetector::new();
    for pid in detector.get_claude_pids() {
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
) {
    thread::spawn(move || {
        let mut was_inside = false;

        loop {
            if let Some((mouse_x, mouse_y_from_top)) =
                current_mouse_position_from_top_left(screen_top_macos_y)
            {
                let left = center_x - hover_width / 2.0;
                let right = left + hover_width;
                // Hysteresis:
                // - use small top strip for initial open trigger
                // - once open, keep panel open across full expanded height so clicks work
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
                    let _ = if inside {
                        app.emit("open-panel", ())
                    } else {
                        app.emit("close-panel", ())
                    };
                    was_inside = inside;
                }
            }

            thread::sleep(Duration::from_millis(16));
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
            ])),
        })
        .setup(|app| {
            // Install hooks if enabled and spawn the socket server
            let app_handle = app.handle().clone();
            if let Some(resource_path) = app.path().resolve("resources/notchai-hook.py", tauri::path::BaseDirectory::Resource).ok() {
                if let Err(e) = hook_installer::install_hooks_if_enabled(&resource_path) {
                    eprintln!("[hooks] install failed: {}", e);
                }
            } else {
                eprintln!("[hooks] could not resolve notchai-hook.py resource path");
            }

            // Spawn the hook socket server as a tokio task
            let server_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                hook_server::start(server_handle).await;
            });

            let window = app.get_webview_window("main").unwrap();

            // Detect notch and position an invisible hover zone over it.
            // The zone is larger than the notch so the mouse can be
            // detected approaching from the sides or below.
            // Iterates all screens to find the one with a physical notch,
            // so this works even when an external monitor is primary.
            let detection = notch::detect_notch();
            let notch = detection.info;
            let hover_width = (notch.width + 340.0).max(540.0);
            // Debug-first sizing: keep the window tall so expanded content is never clipped.
            let hover_height = 320.0;
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
                start_global_hover_monitor(
                    app.handle().clone(),
                    notch.center_x(),
                    hover_width,
                    24.0,
                    320.0,
                    detection.screen_top_macos_y,
                );
            }

            // Force truly transparent window on macOS and place it above the menu bar
            // so the hover zone can receive mouse events (otherwise the menu bar is on top).
            #[cfg(target_os = "macos")]
            {
                use objc::runtime::{Object, NO, YES};
                use objc::{class, msg_send, sel, sel_impl};

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
            }

            // Make visible on all workspaces
            window.set_visible_on_all_workspaces(true).ok();

            window.show().ok();

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_sessions,
            get_notch_info,
            open_session_location,
            resume_session,
            respond_to_approval,
            toggle_hooks_enabled,
            get_hooks_enabled
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
