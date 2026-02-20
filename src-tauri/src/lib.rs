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

            // Detect notch and position an invisible hover zone over it.
            // The zone is larger than the notch so the mouse can be
            // detected approaching from the sides or below.
            let notch = notch::detect_notch();
            let hover_width = (notch.width + 200.0).max(400.0);
            let hover_height = 50.0;
            let x = notch.center_x() - hover_width / 2.0;

            window
                .set_position(tauri::LogicalPosition::new(x, 0.0))
                .ok();
            window
                .set_size(tauri::LogicalSize::new(hover_width, hover_height))
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
