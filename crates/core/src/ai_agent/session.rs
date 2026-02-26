//! Agent Session Management
//!
//! 职责:
//! - 管理 Agent 会话状态
//! - 跟踪对话历史
//! - 记录工具调用统计

use super::{AgentExecutionEvent, AgentToolCall};
use crate::TaskId;
use crate::llm::provider::MessageRole;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Project identity for session scoping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectIdentity {
    pub project_id: String,
    pub project_root: PathBuf,
    pub working_dir: PathBuf,
    pub worktree: PathBuf,
}

impl ProjectIdentity {
    /// Resolve project identity from an optional working directory.
    ///
    /// Strategy:
    /// - Git workspace: use earliest root commit hash as stable project id.
    /// - Non-git workspace: use sha256(canonical root path).
    pub fn detect(working_dir: Option<PathBuf>) -> Self {
        let fallback_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let working_dir = working_dir.unwrap_or(fallback_cwd);
        let working_dir = canonicalize_or_fallback(&working_dir);

        if let Some(project_root) = git_show_toplevel(&working_dir) {
            let project_root = canonicalize_or_fallback(&project_root);
            let worktree =
                git_common_worktree(&project_root).unwrap_or_else(|| project_root.clone());
            let project_id = git_project_id(&project_root)
                .unwrap_or_else(|| sha256_path_fingerprint(&project_root));
            return Self {
                project_id,
                project_root,
                working_dir,
                worktree,
            };
        }

        let project_root = working_dir.clone();
        let project_id = sha256_path_fingerprint(&project_root);
        Self {
            project_id,
            project_root: project_root.clone(),
            working_dir,
            worktree: project_root,
        }
    }
}

fn canonicalize_or_fallback(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn run_git(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn git_show_toplevel(working_dir: &Path) -> Option<PathBuf> {
    let top = run_git(working_dir, &["rev-parse", "--show-toplevel"])?;
    Some(canonicalize_or_fallback(Path::new(&top)))
}

fn git_common_worktree(project_root: &Path) -> Option<PathBuf> {
    let raw = run_git(project_root, &["rev-parse", "--git-common-dir"])?;
    let common_dir = PathBuf::from(raw);
    let worktree = if common_dir.is_absolute() {
        common_dir
    } else {
        project_root.join(common_dir)
    };
    let worktree = if worktree.file_name().is_some_and(|name| name == ".git") {
        worktree
            .parent()
            .map_or_else(|| project_root.to_path_buf(), |p| p.to_path_buf())
    } else {
        worktree
    };
    Some(canonicalize_or_fallback(&worktree))
}

fn git_project_id(project_root: &Path) -> Option<String> {
    let roots = run_git(project_root, &["rev-list", "--max-parents=0", "--all"])?;
    let mut items: Vec<String> = roots
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    items.sort();
    items.into_iter().next()
}

fn sha256_path_fingerprint(path: &Path) -> String {
    let canonical = canonicalize_or_fallback(path);
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Agent 会话
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// 执行事件时间线（用于多轮回放）
    pub execution_events: Vec<AgentExecutionEvent>,

    /// 当前状态
    pub state: SessionState,

    /// 用户元数据
    pub user_metadata: HashMap<String, String>,

    /// Project identity (used for project-scoped session continuity)
    pub project_id: String,

    /// Project root directory
    pub project_root: PathBuf,

    /// Session working directory
    pub working_dir: PathBuf,

    /// Worktree root (git common dir parent when available)
    pub worktree: PathBuf,
}

/// 会话状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// 对应的 tool_call_id (如果是 Tool 消息，用于 LLM request 重建)
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

impl AgentSession {
    /// 创建新会话
    pub fn new(id: String) -> Self {
        let identity = ProjectIdentity::detect(None);
        Self::new_with_project_identity(id, identity)
    }

    /// 创建带项目上下文的新会话
    pub fn new_with_project_identity(id: String, identity: ProjectIdentity) -> Self {
        Self {
            id,
            started_at: chrono::Utc::now(),
            messages: Vec::new(),
            active_tasks: Vec::new(),
            tool_calls: HashMap::new(),
            execution_events: Vec::new(),
            state: SessionState::Idle,
            user_metadata: HashMap::new(),
            project_id: identity.project_id,
            project_root: identity.project_root,
            working_dir: identity.working_dir,
            worktree: identity.worktree,
        }
    }

    /// Merge latest project context into this session.
    pub fn merge_project_identity(&mut self, identity: ProjectIdentity) {
        self.project_id = identity.project_id;
        self.project_root = identity.project_root;
        self.working_dir = identity.working_dir;
        self.worktree = identity.worktree;
    }

    /// 添加消息
    pub fn add_message(&mut self, message: AgentMessage) {
        self.messages.push(message);
    }

    /// 记录工具调用
    pub fn record_tool_call(&mut self, tool_name: &str) {
        *self.tool_calls.entry(tool_name.to_string()).or_insert(0) += 1;
    }

    /// 记录单条执行事件
    pub fn add_execution_event(&mut self, event: AgentExecutionEvent) {
        self.execution_events.push(event);
    }

    /// 追加执行事件
    pub fn add_execution_events(&mut self, events: Vec<AgentExecutionEvent>) {
        self.execution_events.extend(events);
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
        let session =
            AgentSession::new_with_project_identity(id.clone(), ProjectIdentity::detect(None));
        self.sessions.insert(id, session.clone());
        session
    }

    /// Create a new session with an explicit project identity.
    pub fn create_session_with_project_identity(
        &mut self,
        identity: ProjectIdentity,
    ) -> AgentSession {
        let id = ulid::Ulid::new().to_string();
        let session = AgentSession::new_with_project_identity(id.clone(), identity);
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
        let expired_ids: Vec<String> = self
            .sessions
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
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_agent_session_new() {
        let session = AgentSession::new("test-session".to_string());
        assert_eq!(session.id, "test-session");
        assert_eq!(session.state, SessionState::Idle);
        assert!(session.messages.is_empty());
        assert!(session.active_tasks.is_empty());
        assert!(session.execution_events.is_empty());
        assert!(!session.project_id.is_empty());
    }

    #[test]
    fn test_agent_session_new_with_project_identity() {
        let identity = ProjectIdentity {
            project_id: "project-1".to_string(),
            project_root: PathBuf::from("/tmp/project-1"),
            working_dir: PathBuf::from("/tmp/project-1/src"),
            worktree: PathBuf::from("/tmp/project-1"),
        };

        let session =
            AgentSession::new_with_project_identity("test-session".to_string(), identity.clone());
        assert_eq!(session.project_id, identity.project_id);
        assert_eq!(session.project_root, identity.project_root);
        assert_eq!(session.working_dir, identity.working_dir);
        assert_eq!(session.worktree, identity.worktree);
    }

    #[test]
    fn test_project_identity_detect_non_git_path_is_stable() {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let base = std::env::temp_dir().join(format!("ndc-project-identity-{}", millis));
        std::fs::create_dir_all(&base).unwrap();
        let first = ProjectIdentity::detect(Some(base.clone()));
        let second = ProjectIdentity::detect(Some(base.clone()));

        assert!(!first.project_id.is_empty());
        assert_eq!(first.project_id, second.project_id);
        assert!(first.project_root.exists());

        std::fs::remove_dir_all(&base).ok();
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
            tool_call_id: None,
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
        assert!(!session.project_id.is_empty());
        assert_eq!(manager.sessions.len(), 1);
    }

    #[test]
    fn test_session_manager_create_with_project_identity() {
        let mut manager = SessionManager::new();
        let identity = ProjectIdentity {
            project_id: "project-abc".to_string(),
            project_root: PathBuf::from("/tmp/proj"),
            working_dir: PathBuf::from("/tmp/proj"),
            worktree: PathBuf::from("/tmp/proj"),
        };
        let session = manager.create_session_with_project_identity(identity.clone());
        assert_eq!(session.project_id, identity.project_id);
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
