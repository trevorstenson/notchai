use crate::adapter::AgentAdapter;
use crate::models::{AgentSession, AgentType};

pub struct CodexAdapter;

impl CodexAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl AgentAdapter for CodexAdapter {
    fn agent_type(&self) -> AgentType {
        AgentType::Codex
    }

    fn name(&self) -> &str {
        "Codex CLI"
    }

    fn get_sessions(&self) -> Vec<AgentSession> {
        // TODO: Implement Codex session discovery
        // Session path: ~/.codex/sessions/{YYYY}/{MM}/{DD}/rollout-*.jsonl
        // Process detection: `codex` in ps output
        // Format: JSONL with timestamps, message types, token usage
        Vec::new()
    }
}
