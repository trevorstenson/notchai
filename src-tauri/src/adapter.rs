use crate::models::{AgentSession, AgentType};

pub trait AgentAdapter: Send + Sync {
    fn agent_type(&self) -> AgentType;
    fn name(&self) -> &str;
    fn get_sessions(&self) -> Vec<AgentSession>;
}
