use crate::adapter::AgentAdapter;
use crate::models::{AgentSession, AgentStatus};

pub struct AgentMonitor {
    adapters: Vec<Box<dyn AgentAdapter>>,
}

impl AgentMonitor {
    pub fn new(adapters: Vec<Box<dyn AgentAdapter>>) -> Self {
        Self { adapters }
    }

    pub fn get_sessions(&self) -> Vec<AgentSession> {
        let mut all_sessions: Vec<AgentSession> = self
            .adapters
            .iter()
            .flat_map(|adapter| adapter.get_sessions())
            .collect();

        // Active sessions first, then by modified date
        all_sessions.sort_by(|a, b| {
            let a_active = a.status != AgentStatus::Completed;
            let b_active = b.status != AgentStatus::Completed;
            b_active
                .cmp(&a_active)
                .then(b.modified.cmp(&a.modified))
        });

        all_sessions
    }
}
