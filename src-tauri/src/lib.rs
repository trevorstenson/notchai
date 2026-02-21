mod models;
mod monitor;
mod notch;
mod process;
mod scanner;
mod transcript;

use std::sync::Mutex;
use std::process::Command;
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::{thread, time::Duration};

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
        if app_is_running("Warp") && resume_in_warp(trimmed_id, &path_for_open).is_ok() {
            return Ok(());
        }
        if app_is_running("iTerm2") && resume_in_iterm(trimmed_id, &path_for_open).is_ok() {
            return Ok(());
        }
        if app_is_running("Terminal") && resume_in_terminal(trimmed_id, &path_for_open).is_ok() {
            return Ok(());
        }

        if resume_in_warp(trimmed_id, &path_for_open).is_ok()
            || resume_in_iterm(trimmed_id, &path_for_open).is_ok()
            || resume_in_terminal(trimmed_id, &path_for_open).is_ok()
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

#[cfg(target_os = "macos")]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
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
fn resume_in_iterm(session_id: &str, project_path: &str) -> Result<(), String> {
    let command = resume_command(session_id, project_path);
    let script = r#"
on run argv
  set cmd to item 1 of argv
  tell application "iTerm2"
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
) {
    thread::spawn(move || {
        let mut was_inside = false;

        loop {
            if let Some((mouse_x, mouse_y_from_top)) = current_mouse_position_from_top_left() {
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

            thread::sleep(Duration::from_millis(80));
        }
    });
}

#[cfg(target_os = "macos")]
fn current_mouse_position_from_top_left() -> Option<(f64, f64)> {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    unsafe {
        let ns_event_class = Class::get("NSEvent")?;
        let ns_screen_class = Class::get("NSScreen")?;

        let mouse: CGPoint = msg_send![ns_event_class, mouseLocation];
        let main_screen: *mut Object = msg_send![ns_screen_class, mainScreen];
        if main_screen.is_null() {
            return None;
        }

        let frame: CGRect = msg_send![main_screen, frame];
        let from_top = frame.size.height - mouse.y;
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

            #[cfg(target_os = "macos")]
            {
                // Global hover monitor so open/close works even when another app is focused.
                start_global_hover_monitor(
                    app.handle().clone(),
                    notch.center_x(),
                    hover_width,
                    60.0,
                    320.0,
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
            resume_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
