use std::process::Command;
use std::time::{Duration, SystemTime};

pub struct ProcessDetector;

impl ProcessDetector {
    pub fn new() -> Self {
        Self
    }

    pub fn is_any_claude_running(&self) -> bool {
        !self.get_claude_pids().is_empty()
    }

    pub fn get_claude_pids(&self) -> Vec<u32> {
        let output = match Command::new("ps").args(["-eo", "pid,args"]).output() {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut pids = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            if (line.contains("/claude") || line.contains("claude ")) && !line.contains("grep") {
                if let Some(pid_str) = line.split_whitespace().next() {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        pids.push(pid);
                    }
                }
            }
        }

        pids
    }

    pub fn is_session_active(&self, jsonl_path: &str) -> bool {
        self.get_jsonl_age_secs(jsonl_path)
            .map_or(false, |age| age < 900)
    }

    pub fn get_jsonl_age_secs(&self, jsonl_path: &str) -> Option<u64> {
        let metadata = std::fs::metadata(jsonl_path).ok()?;
        let modified = metadata.modified().ok()?;
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or(Duration::from_secs(u64::MAX));
        Some(age.as_secs())
    }
}
