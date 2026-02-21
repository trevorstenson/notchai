use std::fs;
use std::path::PathBuf;
use std::collections::HashSet;

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
        let mut seen_session_ids: HashSet<String> = HashSet::new();

        let dir = match fs::read_dir(&self.claude_dir) {
            Ok(d) => d,
            Err(err) => {
                eprintln!(
                    "[notchai] scan_all_projects read_dir failed path={} err={}",
                    self.claude_dir.display(),
                    err
                );
                return entries;
            }
        };

        for entry in dir.flatten() {
            if entry.file_type().map_or(false, |ft| ft.is_dir()) {
                let project_dir = entry.path();
                let index_path = project_dir.join("sessions-index.json");
                if index_path.exists() {
                    if let Ok(content) = fs::read_to_string(&index_path) {
                        if let Ok(index) = serde_json::from_str::<SessionIndexFile>(&content) {
                            for item in index.entries {
                                seen_session_ids.insert(item.session_id.clone());
                                entries.push(item);
                            }
                        }
                    }
                }

                // Fallback: some Claude project dirs may have JSONL files without a
                // sessions-index.json entry. Include those so active sessions still surface.
                if let Ok(project_items) = fs::read_dir(&project_dir) {
                    for item in project_items.flatten() {
                        if !item.file_type().map_or(false, |ft| ft.is_file()) {
                            continue;
                        }

                        let path = item.path();
                        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                            continue;
                        }

                        let session_id = match path.file_stem().and_then(|s| s.to_str()) {
                            Some(id) => id.to_string(),
                            None => continue,
                        };

                        if seen_session_ids.contains(&session_id) {
                            continue;
                        }

                        let full_path = path.to_string_lossy().to_string();
                        let modified = fs::metadata(&path)
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .map(|ts| chrono::DateTime::<chrono::Utc>::from(ts).to_rfc3339());

                        entries.push(SessionIndexEntry {
                            session_id: session_id.clone(),
                            full_path,
                            first_prompt: None,
                            summary: None,
                            message_count: None,
                            created: modified.clone(),
                            modified,
                            git_branch: None,
                            project_path: None,
                            is_sidechain: None,
                        });
                        seen_session_ids.insert(session_id);
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

        eprintln!(
            "[notchai] scan_all_projects path={} total_entries={}",
            self.claude_dir.display(),
            entries.len()
        );

        entries
    }
}
