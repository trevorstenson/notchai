mod models;
mod monitor;
mod notch;
mod process;
mod scanner;
mod transcript;

use std::sync::Mutex;

use models::{AgentSession, NotchInfo};
use monitor::AgentMonitor;
use tauri::Manager;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            monitor: Mutex::new(AgentMonitor::new()),
        })
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            // Detect notch and position window
            let notch = notch::detect_notch();
            let collapsed_width = (notch.width + 100.0).max(280.0);
            let collapsed_height = 40.0;
            let x = notch.center_x() - collapsed_width / 2.0;

            window
                .set_position(tauri::LogicalPosition::new(x, 0.0))
                .ok();
            window
                .set_size(tauri::LogicalSize::new(collapsed_width, collapsed_height))
                .ok();

            // Make visible on all workspaces
            window.set_visible_on_all_workspaces(true).ok();

            window.show().ok();

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_sessions, get_notch_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
