use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The 9 Claude Code hook event types we register for.
const HOOK_EVENT_TYPES: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "UserPromptSubmit",
    "Notification",
    "Stop",
    "SubagentStop",
    "SessionStart",
    "SessionEnd",
];

/// Marker used to identify our hook entries in settings.json.
const NOTCHAI_HOOK_MARKER: &str = "notchai-hook.py";

/// Timeout for the PermissionRequest hook (seconds).
const PERMISSION_TIMEOUT: u64 = 86400;

/// Destination path for the installed hook script.
fn hook_script_dest() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("hooks").join("notchai-hook.py"))
}

/// Path to Claude Code settings.json.
fn claude_settings_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("settings.json"))
}

/// Path to Notchai config.json.
fn notchai_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".notchai").join("config.json"))
}

/// Detect whether `python3` or `python` is available, returning the command name.
fn detect_python_command() -> String {
    if Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        "python3".to_string()
    } else {
        "python".to_string()
    }
}

/// Build a hook entry object for a given event type.
fn build_hook_entry(python_cmd: &str, script_path: &str, event_type: &str) -> Value {
    let mut hook = Map::new();
    hook.insert("type".to_string(), json!("command"));
    hook.insert(
        "command".to_string(),
        json!(format!("{} {}", python_cmd, script_path)),
    );
    if event_type == "PermissionRequest" {
        hook.insert("timeout".to_string(), json!(PERMISSION_TIMEOUT));
    }

    json!({
        "matcher": "",
        "hooks": [Value::Object(hook)]
    })
}

/// Install hooks: copy the script and update settings.json.
///
/// `source_script` is the path to notchai-hook.py in the app bundle / resources.
pub fn install_hooks(source_script: &Path) -> Result<(), String> {
    let dest = hook_script_dest().ok_or("Cannot determine home directory")?;
    let settings_path = claude_settings_path().ok_or("Cannot determine home directory")?;

    // Create destination directory
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create hooks directory: {}", e))?;
    }

    // Copy the hook script
    fs::copy(source_script, &dest)
        .map_err(|e| format!("Failed to copy hook script: {}", e))?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&dest, perms)
            .map_err(|e| format!("Failed to set script permissions: {}", e))?;
    }

    // Detect python command
    let python_cmd = detect_python_command();
    let script_path_str = dest.to_string_lossy().to_string();

    // Read or create settings.json
    let mut settings: Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)
            .map_err(|e| format!("Failed to read settings.json: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse settings.json: {}", e))?
    } else {
        // Create parent dir if needed
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create .claude directory: {}", e))?;
        }
        json!({})
    };

    // Ensure settings is an object
    let settings_obj = settings
        .as_object_mut()
        .ok_or("settings.json is not a JSON object")?;

    // Get or create the hooks section
    if !settings_obj.contains_key("hooks") {
        settings_obj.insert("hooks".to_string(), json!({}));
    }
    let hooks = settings_obj
        .get_mut("hooks")
        .and_then(|v| v.as_object_mut())
        .ok_or("hooks section is not a JSON object")?;

    // Merge hook entries for each event type (idempotent)
    for event_type in HOOK_EVENT_TYPES {
        let entry = build_hook_entry(&python_cmd, &script_path_str, event_type);

        if let Some(existing_array) = hooks.get_mut(*event_type) {
            if let Some(arr) = existing_array.as_array_mut() {
                // Check if we already have a notchai hook entry
                let has_notchai = arr.iter().any(|item| {
                    item.get("hooks")
                        .and_then(|h| h.as_array())
                        .map(|hooks_arr| {
                            hooks_arr.iter().any(|hook| {
                                hook.get("command")
                                    .and_then(|c| c.as_str())
                                    .map(|cmd| cmd.contains(NOTCHAI_HOOK_MARKER))
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false)
                });

                if !has_notchai {
                    arr.push(entry);
                } else {
                    // Update existing entry in place (e.g. python command may have changed)
                    for item in arr.iter_mut() {
                        let is_notchai = item
                            .get("hooks")
                            .and_then(|h| h.as_array())
                            .map(|hooks_arr| {
                                hooks_arr.iter().any(|hook| {
                                    hook.get("command")
                                        .and_then(|c| c.as_str())
                                        .map(|cmd| cmd.contains(NOTCHAI_HOOK_MARKER))
                                        .unwrap_or(false)
                                })
                            })
                            .unwrap_or(false);
                        if is_notchai {
                            *item = entry.clone();
                            break;
                        }
                    }
                }
            }
        } else {
            hooks.insert(event_type.to_string(), json!([entry]));
        }
    }

    // Write settings back
    let output = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    fs::write(&settings_path, output)
        .map_err(|e| format!("Failed to write settings.json: {}", e))?;

    Ok(())
}

/// Remove hook entries from settings.json and delete the script file.
pub fn uninstall_hooks() -> Result<(), String> {
    let settings_path = claude_settings_path().ok_or("Cannot determine home directory")?;

    // Remove hook entries from settings.json
    if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)
            .map_err(|e| format!("Failed to read settings.json: {}", e))?;
        let mut settings: Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse settings.json: {}", e))?;

        if let Some(hooks) = settings
            .as_object_mut()
            .and_then(|obj| obj.get_mut("hooks"))
            .and_then(|v| v.as_object_mut())
        {
            for event_type in HOOK_EVENT_TYPES {
                if let Some(arr_val) = hooks.get_mut(*event_type) {
                    if let Some(arr) = arr_val.as_array_mut() {
                        arr.retain(|item| {
                            !item
                                .get("hooks")
                                .and_then(|h| h.as_array())
                                .map(|hooks_arr| {
                                    hooks_arr.iter().any(|hook| {
                                        hook.get("command")
                                            .and_then(|c| c.as_str())
                                            .map(|cmd| cmd.contains(NOTCHAI_HOOK_MARKER))
                                            .unwrap_or(false)
                                    })
                                })
                                .unwrap_or(false)
                        });
                        // Remove the event type key entirely if the array is now empty
                        if arr.is_empty() {
                            // Mark for removal (can't remove while iterating)
                        }
                    }
                }
            }

            // Clean up empty arrays
            let empty_keys: Vec<String> = hooks
                .iter()
                .filter(|(_, v)| v.as_array().map(|a| a.is_empty()).unwrap_or(false))
                .map(|(k, _)| k.clone())
                .collect();
            for key in empty_keys {
                hooks.remove(&key);
            }
        }

        let output = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        fs::write(&settings_path, output)
            .map_err(|e| format!("Failed to write settings.json: {}", e))?;
    }

    // Delete the hook script file
    if let Some(dest) = hook_script_dest() {
        if dest.exists() {
            fs::remove_file(&dest)
                .map_err(|e| format!("Failed to delete hook script: {}", e))?;
        }
    }

    Ok(())
}

/// Check if hooks are enabled in the Notchai config, then install if so.
///
/// `source_script` is the path to notchai-hook.py in the app bundle / resources.
pub fn install_hooks_if_enabled(source_script: &Path) -> Result<(), String> {
    if get_hooks_enabled() {
        install_hooks(source_script)
    } else {
        Ok(())
    }
}

/// Read the hooks_enabled flag from ~/.notchai/config.json. Defaults to true.
pub fn get_hooks_enabled() -> bool {
    let Some(config_path) = notchai_config_path() else {
        return true;
    };
    if !config_path.exists() {
        return true;
    }
    let Ok(content) = fs::read_to_string(&config_path) else {
        return true;
    };
    let Ok(config) = serde_json::from_str::<Value>(&content) else {
        return true;
    };
    config
        .get("hooks_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Set the hooks_enabled flag in ~/.notchai/config.json.
pub fn set_hooks_enabled(enabled: bool) -> Result<(), String> {
    let config_path = notchai_config_path().ok_or("Cannot determine home directory")?;

    // Read existing config or start fresh
    let mut config: Value = if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        serde_json::from_str(&content).unwrap_or(json!({}))
    } else {
        json!({})
    };

    // Create parent dir if needed
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    // Update the flag
    if let Some(obj) = config.as_object_mut() {
        obj.insert("hooks_enabled".to_string(), json!(enabled));
    }

    let output = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&config_path, output)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}

/// Read the sound_enabled flag from ~/.notchai/config.json. Defaults to true.
pub fn get_sound_enabled() -> bool {
    let Some(config_path) = notchai_config_path() else {
        return true;
    };
    if !config_path.exists() {
        return true;
    }
    let Ok(content) = fs::read_to_string(&config_path) else {
        return true;
    };
    let Ok(config) = serde_json::from_str::<Value>(&content) else {
        return true;
    };
    config
        .get("sound_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Set the sound_enabled flag in ~/.notchai/config.json.
pub fn set_sound_enabled(enabled: bool) -> Result<(), String> {
    let config_path = notchai_config_path().ok_or("Cannot determine home directory")?;

    let mut config: Value = if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        serde_json::from_str(&content).unwrap_or(json!({}))
    } else {
        json!({})
    };

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    if let Some(obj) = config.as_object_mut() {
        obj.insert("sound_enabled".to_string(), json!(enabled));
    }

    let output = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&config_path, output)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}

/// Read the selected_screen from ~/.notchai/config.json. Defaults to None (auto-detect).
pub fn get_selected_screen() -> Option<usize> {
    let config_path = notchai_config_path()?;
    if !config_path.exists() {
        return None;
    }
    let content = fs::read_to_string(&config_path).ok()?;
    let config: Value = serde_json::from_str(&content).ok()?;
    config
        .get("selected_screen")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
}

/// Set the selected_screen in ~/.notchai/config.json.
pub fn set_selected_screen(index: Option<usize>) -> Result<(), String> {
    let config_path = notchai_config_path().ok_or("Cannot determine home directory")?;

    let mut config: Value = if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        serde_json::from_str(&content).unwrap_or(json!({}))
    } else {
        json!({})
    };

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    if let Some(obj) = config.as_object_mut() {
        match index {
            Some(i) => obj.insert("selected_screen".to_string(), json!(i)),
            None => obj.insert("selected_screen".to_string(), Value::Null),
        };
    }

    let output = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&config_path, output)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}
