use std::process::Command;
use std::time::{Duration, SystemTime};

/// A single snapshot of `ps -eo pid,args` output, captured once per poll cycle
/// and shared across all adapters to avoid redundant subprocess spawns.
pub struct ProcessSnapshot {
    pub ps_output: String,
}

impl ProcessSnapshot {
    /// Capture a snapshot by running `ps -eo pid,args` once.
    pub fn capture() -> Self {
        let ps_output = match Command::new("ps").args(["-eo", "pid,args"]).output() {
            Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
            Err(_) => String::new(),
        };
        Self { ps_output }
    }

    /// Returns true if any line in the snapshot matches the predicate.
    pub fn has_process(&self, matcher: impl Fn(&str) -> bool) -> bool {
        self.ps_output.lines().any(|line| matcher(line.trim()))
    }

    /// Returns PIDs of all lines matching the predicate.
    pub fn get_matching_pids(&self, matcher: impl Fn(&str) -> bool) -> Vec<u32> {
        let mut pids = Vec::new();
        for line in self.ps_output.lines() {
            let line = line.trim();
            if matcher(line) {
                if let Some(pid_str) = line.split_whitespace().next() {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        pids.push(pid);
                    }
                }
            }
        }
        pids
    }
}

pub fn get_jsonl_age_secs(jsonl_path: &str) -> Option<u64> {
    let metadata = std::fs::metadata(jsonl_path).ok()?;
    let modified = metadata.modified().ok()?;
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or(Duration::from_secs(u64::MAX));
    Some(age.as_secs())
}

pub fn is_session_active(jsonl_path: &str) -> bool {
    get_jsonl_age_secs(jsonl_path).map_or(false, |age| age < 900)
}
