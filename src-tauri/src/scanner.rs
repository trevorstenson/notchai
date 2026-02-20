use std::fs;
use std::path::PathBuf;

use crate::models::{SessionIndexEntry, SessionIndexFile};

pub struct SessionIndexScanner {
    claude_dir: PathBuf,
}

impl SessionIndexScanner {
    pub fn new() -> Self {
        let home = dirs::home_dir().expect("Could not find home directory");
        Self {
            claude_dir: home.join(".claude").join("projects"),
        }
    }

    pub fn scan_all_projects(&self) -> Vec<SessionIndexEntry> {
        let mut entries = Vec::new();

        let dir = match fs::read_dir(&self.claude_dir) {
            Ok(d) => d,
            Err(_) => return entries,
        };

        for entry in dir.flatten() {
            if entry.file_type().map_or(false, |ft| ft.is_dir()) {
                let index_path = entry.path().join("sessions-index.json");
                if index_path.exists() {
                    if let Ok(content) = fs::read_to_string(&index_path) {
                        if let Ok(index) = serde_json::from_str::<SessionIndexFile>(&content) {
                            entries.extend(index.entries);
                        }
                    }
                }
            }
        }

        // Sort by modified date descending
        entries.sort_by(|a, b| {
            let a_mod = a.modified.as_deref().unwrap_or("");
            let b_mod = b.modified.as_deref().unwrap_or("");
            b_mod.cmp(a_mod)
        });

        entries
    }
}
