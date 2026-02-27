use crate::adapter::AgentAdapter;
use crate::models::{AgentSession, AgentType};

pub struct CursorAdapter;

impl CursorAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl AgentAdapter for CursorAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Cursor
    }

    fn name(&self) -> &str {
        "Cursor"
    }

    fn get_sessions(&self) -> Vec<AgentSession> {
        // TODO: Implement Cursor session discovery
        // Transcripts: ~/.cursor/projects/{path}/agent-transcripts/{uuid}.txt
        // Tracking: ~/.cursor/ai-tracking/ai-code-tracking.db (SQLite)
        // Process detection: `Cursor` app via pgrep
        Vec::new()
    }
}
