mod models;
mod monitor;
mod notch;
mod process;
mod scanner;
mod transcript;

use std::sync::Mutex;
use std::process::Command;
use std::path::PathBuf;

use models::{AgentSession, NotchInfo};
use monitor::AgentMonitor;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::ShortcutState;

struct AppState {
    monitor: Mutex<AgentMonitor>,
}

#[tauri::command]
fn get_sessions(state: tauri::State<'_, AppState>) -> Vec<AgentSession> {
    state.monitor.lock().unwrap().get_sessions()
}

#[tauri::command]
fn get_notch_info() -> NotchInfo {
    notch::detect_notch()
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
        // Warp-first behavior: only bring Warp forward, do not open new tabs.
        if focus_warp_app().is_ok() {
            return Ok(());
        }
        if app_is_running("iTerm2") {
            if open_in_iterm(&path_for_open).is_ok() {
                return Ok(());
            }
        }
        if app_is_running("Terminal") {
            if open_in_terminal(&path_for_open).is_ok() {
                return Ok(());
            }
        }

        // Fallback chain if no preferred terminal is running.
        if open_in_iterm(&path_for_open).is_ok()
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
fn open_in_iterm(project_path: &str) -> Result<(), String> {
    let script = r#"
on run argv
  set p to item 1 of argv
  tell application "iTerm2"
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
        .arg(project_path)
        .status()
        .map_err(|e| format!("iterm osascript failed: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err("iterm osascript returned non-zero".to_string())
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
        .plugin(shortcut_plugin)
        .manage(AppState {
            monitor: Mutex::new(AgentMonitor::new()),
        })
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            // Detect notch and position an invisible hover zone over it.
            // The zone is larger than the notch so the mouse can be
            // detected approaching from the sides or below.
            let notch = notch::detect_notch();
            let hover_width = (notch.width + 340.0).max(540.0);
            // Debug-first sizing: keep the window tall so expanded content is never clipped.
            let hover_height = 320.0;
            let x = notch.center_x() - hover_width / 2.0;

            window
                .set_position(tauri::LogicalPosition::new(x, 0.0))
                .ok();
            window
                .set_size(tauri::LogicalSize::new(hover_width, hover_height))
                .ok();

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
            open_session_location
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
