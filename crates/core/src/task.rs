//! Task 模型 - Task 是持久化的 Intent
//!
//! 整合原则：
//! - Intent: AI 提出的行动提案
//! - Task: 已通过 Verdict 裁决的 Intent，可执行
//! - Task 包含原始 Intent 信息，保证可追溯性
//! - Snapshot 使用 Git Worktree 实现，支持精确回滚

use crate::intent::{Intent, Action, Verdict};
use crate::agent::AgentRole;
use crate::memory::MemoryId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 任务状态
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskState {
    Pending,              // 待处理
    Preparing,            // 准备中
    InProgress,           // 进行中
    AwaitingVerification, // 等待验证
    Blocked,              // 被阻塞
    Completed,            // 已完成
    Failed,               // 失败
    Cancelled,            // 已取消
}

/// Git Worktree 快照（使用 git worktree 实现精确回滚）
///
/// 工作原理：
/// 1. 在 Preparing 阶段创建临时 worktree
/// 2. 在 worktree 中执行操作
/// 3. 回滚时删除 worktree 并切换回主分支
/// 4. 不会污染主分支历史
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitWorktreeSnapshot {
    /// 快照 ID
    pub id: SnapshotId,

    /// Worktree 路径
    pub worktree_path: PathBuf,

    /// 创建时间
    pub created_at: Timestamp,

    /// 创建时的 commit hash
    pub base_commit: String,

    /// 此快照对应的分支名
    pub branch_name: String,

    /// 任务 ID（关联）
    pub task_id: TaskId,

    /// 执行的步骤 ID
    pub step_id: Option<u64>,

    /// 受影响的文件列表（相对路径）
    pub affected_paths: Vec<PathBuf>,

    /// 操作摘要
    pub operation_summary: String,
}

/// 文件变更记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// 文件路径（相对）
    pub path: PathBuf,

    /// 变更类型
    pub change_type: ChangeType,

    /// 变更前的内容 hash
    pub old_content_hash: Option<String>,

    /// 变更后的内容 hash
    pub new_content_hash: Option<String>,

    /// 行数统计
    pub lines_added: u32,
    pub lines_removed: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// 轻量级快照（仅记录路径，不创建 worktree）
/// 用于不需要回滚的场景
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightweightSnapshot {
    pub id: SnapshotId,
    pub captured_at: Timestamp,
    pub paths: Vec<PathBuf>,
    pub checksum: String,  // SHA256 of all file contents
}

/// 任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// 任务 ID
    pub id: TaskId,

    /// 原始 Intent（追溯来源）- 可选，用于向后兼容
    pub intent: Option<Intent>,

    /// 裁决结果（证明合法性）- 可选
    pub verdict: Option<Verdict>,

    /// 当前状态
    pub state: TaskState,

    /// 任务标题
    pub title: String,

    /// 任务描述
    pub description: String,

    /// 允许的状态转换
    #[serde(default)]
    pub allowed_transitions: Vec<TaskState>,

    /// 执行步骤
    #[serde(default)]
    pub steps: Vec<ExecutionStep>,

    /// 质量门禁
    pub quality_gate: Option<QualityGate>,

    /// Git Worktree 快照（Preparing/InProgress 阶段创建）
    #[serde(default)]
    pub snapshots: Vec<GitWorktreeSnapshot>,

    /// 轻量级快照（不需要回滚时使用）
    #[serde(default)]
    pub lightweight_snapshots: Vec<LightweightSnapshot>,

    /// 元数据
    pub metadata: TaskMetadata,
}

/// 执行步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub step_id: u64,
    pub action: Action,
    pub status: StepStatus,
    pub result: Option<ActionResult>,
    pub executed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub metrics: ActionMetrics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionMetrics {
    pub duration_ms: u64,
    pub tokens_used: u64,
    pub memory_access: Vec<MemoryId>,
}

/// 质量门禁
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGate {
    pub checks: Vec<QualityCheck>,
    pub strategy: GateStrategy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GateStrategy {
    FailFast,
    AllMustPass,
    Weighted,
}

/// 质量检查
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityCheck {
    pub check_type: QualityCheckType,
    pub command: Option<String>,
    pub pass_condition: PassCondition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QualityCheckType {
    Test,
    Lint,
    TypeCheck,
    Build,
    Security,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PassCondition {
    ExitCode(u32),
    RegexMatch(String),
    OutputContains(String),
}

/// 任务元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetadata {
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub created_by: AgentRole,
    pub priority: TaskPriority,
    pub tags: Vec<String>,
    pub work_records: Vec<WorkRecord>,
}

impl Default for TaskMetadata {
    fn default() -> Self {
        let now = chrono::Utc::now();
        Self {
            created_at: now,
            updated_at: now,
            created_by: AgentRole::Historian,
            priority: TaskPriority::Medium,
            tags: vec![],
            work_records: vec![],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Critical,
}

impl Task {
    /// 创建新任务（从 Intent 和 Verdict）
    pub fn from_intent_and_verdict(intent: Intent, verdict: Verdict) -> Self {
        Self {
            id: TaskId::new(),
            intent: Some(intent),
            verdict: Some(verdict),
            state: TaskState::Pending,
            title: String::new(),
            description: String::new(),
            allowed_transitions: Self::initial_transitions(),
            steps: vec![],
            quality_gate: None,
            snapshots: vec![],
            lightweight_snapshots: vec![],
            metadata: TaskMetadata::default(),
        }
    }

    /// 创建简单任务（向后兼容）
    pub fn new(title: String, description: String, created_by: AgentRole) -> Self {
        Self {
            id: TaskId::new(),
            intent: None,
            verdict: None,
            state: TaskState::Pending,
            title,
            description,
            allowed_transitions: Self::initial_transitions(),
            steps: vec![],
            quality_gate: None,
            snapshots: vec![],
            lightweight_snapshots: vec![],
            metadata: TaskMetadata {
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                created_by,
                priority: TaskPriority::Medium,
                tags: vec![],
                work_records: vec![],
            },
        }
    }

    fn initial_transitions() -> Vec<TaskState> {
        vec![TaskState::Preparing]
    }

    /// 请求状态转换
    pub fn request_transition(&mut self, to: TaskState) -> Result<(), TransitionError> {
        if !self.allowed_transitions.contains(&to) {
            return Err(TransitionError::NotAllowed {
                from: self.state.clone(),
                to,
            });
        }
        self.state = to;
        self.update_allowed_transitions();
        self.metadata.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// 更新允许的转换
    fn update_allowed_transitions(&mut self) {
        self.allowed_transitions = match self.state {
            TaskState::Pending => vec![TaskState::Preparing],
            TaskState::Preparing => vec![TaskState::InProgress],
            TaskState::InProgress => vec![
                TaskState::AwaitingVerification,
                TaskState::Blocked,
            ],
            TaskState::AwaitingVerification => vec![
                TaskState::Completed,
                TaskState::Failed,
                TaskState::InProgress,
            ],
            TaskState::Blocked => vec![TaskState::InProgress],
            _ => vec![],
        };
    }

    /// 创建 Git Worktree 快照
    pub fn capture_worktree_snapshot(
        &mut self,
        worktree_path: PathBuf,
        base_commit: String,
        branch_name: String,
        affected_paths: Vec<PathBuf>,
        operation_summary: String,
    ) {
        self.snapshots.push(GitWorktreeSnapshot {
            id: SnapshotId::new(),
            worktree_path,
            created_at: chrono::Utc::now(),
            base_commit,
            branch_name,
            task_id: self.id,
            step_id: self.steps.last().map(|s| s.step_id),
            affected_paths,
            operation_summary,
        });
    }

    /// 创建轻量级快照
    pub fn capture_lightweight_snapshot(&mut self, paths: Vec<PathBuf>, checksum: String) {
        self.lightweight_snapshots.push(LightweightSnapshot {
            id: SnapshotId::new(),
            captured_at: chrono::Utc::now(),
            paths,
            checksum,
        });
    }

    /// 获取最新 Git Worktree 快照
    pub fn latest_worktree_snapshot(&self) -> Option<&GitWorktreeSnapshot> {
        self.snapshots.last()
    }

    /// 获取最新轻量级快照
    pub fn latest_lightweight_snapshot(&self) -> Option<&LightweightSnapshot> {
        self.lightweight_snapshots.last()
    }
}

/// 工作记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkRecord {
    pub id: WorkRecordId,
    pub timestamp: Timestamp,
    pub event: WorkEvent,
    pub executor: Executor,
    pub result: WorkResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkEvent {
    Created,
    Started,
    StepCompleted,
    StepFailed,
    Blocked,
    Unblocked,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Executor {
    Agent(AgentRole),
    Human,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkResult {
    Success,
    Failure(String),
    Pending,
}

/// 错误类型
#[derive(Debug, thiserror::Error)]
pub enum TransitionError {
    #[error("不允许的状态转换: {from:?} -> {to:?}")]
    NotAllowed { from: TaskState, to: TaskState },
}

// 类型别名
pub type TaskId = ulid::Ulid;
pub type SnapshotId = ulid::Ulid;
pub type WorkRecordId = ulid::Ulid;
pub type Timestamp = chrono::DateTime<chrono::Utc>;
