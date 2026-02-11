//! Agent Session Management
//!
//! 职责:
//! - 管理 Agent 会话状态
//! - 跟踪对话历史
//! - 记录工具调用统计

use super::AgentToolCall;
use crate::llm::provider::MessageRole;
use crate::TaskId;
use std::collections::HashMap;

/// Agent 会话
#[derive(Debug, Clone)]
pub struct AgentSession {
    /// 会话 ID
    pub id: String,

    /// 开始时间
    pub started_at: chrono::DateTime<chrono::Utc>,

    /// 对话历史
    pub messages: Vec<AgentMessage>,

    /// 活跃任务
    pub active_tasks: Vec<TaskId>,

    /// 工具调用统计
    pub tool_calls: HashMap<String, usize>,

    /// 当前状态
    pub state: SessionState,

    /// 用户元数据
    pub user_metadata: HashMap<String, String>,
}

/// 会话状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// 空闲
    Idle,

    /// 思考中 (正在等待 LLM 响应)
    Thinking,

    /// 等待权限确认
    WaitingForPermission,

    /// 执行工具中
    Executing,

    /// 验证中
    Verifying,

    /// 已完成
    Completed,

    /// 错误状态
    Error,
}

/// Agent 消息
#[derive(Debug, Clone)]
pub struct AgentMessage {
    /// 消息角色
    pub role: MessageRole,

    /// 消息内容
    pub content: String,

    /// 时间戳
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// 工具调用 (如果是 Assistant 消息)
    pub tool_calls: Option<Vec<AgentToolCall>>,

    /// 工具结果 (如果是 Tool 消息)
    pub tool_results: Option<Vec<String>>,
}

impl AgentSession {
    /// 创建新会话
    pub fn new(id: String) -> Self {
        Self {
            id,
            started_at: chrono::Utc::now(),
            messages: Vec::new(),
            active_tasks: Vec::new(),
            tool_calls: HashMap::new(),
            state: SessionState::Idle,
            user_metadata: HashMap::new(),
        }
    }

    /// 添加消息
    pub fn add_message(&mut self, message: AgentMessage) {
        self.messages.push(message);
    }

    /// 记录工具调用
    pub fn record_tool_call(&mut self, tool_name: &str) {
        *self.tool_calls.entry(tool_name.to_string()).or_insert(0) += 1;
    }

    /// 添加活跃任务
    pub fn add_active_task(&mut self, task_id: TaskId) {
        self.active_tasks.push(task_id);
    }

    /// 设置状态
    pub fn set_state(&mut self, state: SessionState) {
        self.state = state;
    }

    /// 获取持续时间
    pub fn duration(&self) -> chrono::Duration {
        chrono::Utc::now() - self.started_at
    }

    /// 是否超时
    pub fn is_expired(&self, timeout_secs: u64) -> bool {
        self.duration().num_seconds() >= timeout_secs as i64
    }
}

/// Session Manager
#[derive(Debug, Clone)]
pub struct SessionManager {
    sessions: HashMap<String, AgentSession>,
    default_timeout_secs: u64,
}

impl SessionManager {
    /// 创建新的 Session Manager
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            default_timeout_secs: 3600, // 1 hour
        }
    }

    /// 创建新会话
    pub fn create_session(&mut self) -> AgentSession {
        let id = ulid::Ulid::new().to_string();
        let session = AgentSession::new(id.clone());
        self.sessions.insert(id, session.clone());
        session
    }

    /// 获取会话
    pub fn get_session(&self, id: &str) -> Option<&AgentSession> {
        self.sessions.get(id)
    }

    /// 获取可变会话
    pub fn get_session_mut(&mut self, id: &str) -> Option<&mut AgentSession> {
        self.sessions.get_mut(id)
    }

    /// 清理过期会话
    pub fn cleanup_expired(&mut self) {
        let expired_ids: Vec<String> = self.sessions
            .iter()
            .filter(|(_, session)| session.is_expired(self.default_timeout_secs))
            .map(|(id, _)| id.clone())
            .collect();

        for id in expired_ids {
            self.sessions.remove(&id);
        }
    }

    /// 删除会话
    pub fn remove_session(&mut self, id: &str) -> Option<AgentSession> {
        self.sessions.remove(id)
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_session_new() {
        let session = AgentSession::new("test-session".to_string());
        assert_eq!(session.id, "test-session");
        assert_eq!(session.state, SessionState::Idle);
        assert!(session.messages.is_empty());
        assert!(session.active_tasks.is_empty());
    }

    #[test]
    fn test_agent_session_add_message() {
        let mut session = AgentSession::new("test".to_string());
        let message = AgentMessage {
            role: MessageRole::User,
            content: "Hello".to_string(),
            timestamp: chrono::Utc::now(),
            tool_calls: None,
            tool_results: None,
        };

        session.add_message(message);
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].content, "Hello");
    }

    #[test]
    fn test_agent_session_record_tool_call() {
        let mut session = AgentSession::new("test".to_string());
        session.record_tool_call("file_read");
        session.record_tool_call("file_read");
        session.record_tool_call("file_write");

        assert_eq!(session.tool_calls.get("file_read"), Some(&2));
        assert_eq!(session.tool_calls.get("file_write"), Some(&1));
    }

    #[test]
    fn test_agent_session_state() {
        let mut session = AgentSession::new("test".to_string());
        assert_eq!(session.state, SessionState::Idle);

        session.set_state(SessionState::Thinking);
        assert_eq!(session.state, SessionState::Thinking);
    }

    #[test]
    fn test_agent_session_expiration() {
        let session = AgentSession::new("test".to_string());
        assert!(!session.is_expired(3600));
        assert!(session.is_expired(0));
    }

    #[test]
    fn test_session_manager_new() {
        let manager = SessionManager::new();
        assert!(manager.sessions.is_empty());
        assert_eq!(manager.default_timeout_secs, 3600);
    }

    #[test]
    fn test_session_manager_create() {
        let mut manager = SessionManager::new();
        let session = manager.create_session();

        assert!(!session.id.is_empty());
        assert_eq!(manager.sessions.len(), 1);
    }

    #[test]
    fn test_session_manager_get() {
        let mut manager = SessionManager::new();
        let session = manager.create_session();
        let id = session.id.clone();

        let retrieved = manager.get_session(&id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, id);
    }

    #[test]
    fn test_session_manager_remove() {
        let mut manager = SessionManager::new();
        let session = manager.create_session();
        let id = session.id.clone();

        let removed = manager.remove_session(&id);
        assert!(removed.is_some());
        assert!(manager.get_session(&id).is_none());
    }

    #[test]
    fn test_session_manager_cleanup() {
        let mut manager = SessionManager::new();
        manager.default_timeout_secs = 0;

        let session = manager.create_session();
        let id = session.id.clone();

        manager.cleanup_expired();
        assert!(manager.get_session(&id).is_none());
    }
}
