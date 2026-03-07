use crate::models::{AgentSession, AgentType, ToolCallInfo};
use crate::process::ProcessSnapshot;

pub trait AgentAdapter: Send + Sync {
    fn agent_type(&self) -> AgentType;
    fn name(&self) -> &str;
    fn get_sessions(&self, snapshot: &ProcessSnapshot) -> Vec<AgentSession>;
    fn get_tool_calls(&self, _session_id: &str) -> Vec<ToolCallInfo> {
        Vec::new()
    }
}
