//! Task 模型 - Task 是持久化的 Intent
//!
//! 整合原则：
//! - Intent: AI 提出的行动提案
//! - Task: 已通过 Verdict 裁决的 Intent，可执行
//! - Task 包含原始 Intent 信息，保证可追溯性
//! - Task 包含 Snapshot，支持回滚

use crate::intent::{Intent, Action, Verdict};
use crate::agent::AgentRole;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fmt;

/// 任务状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// 任务快照（支持回滚）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    /// 快照 ID
    pub id: SnapshotId,
    /// 捕获时间
    pub captured_at: Timestamp,
    /// Git Commit Hash（如适用）
    pub git_commit: Option<String>,
    /// 受影响文件列表（路径 + hash）
    pub affected_files: Vec<FileSnapshot>,
    /// 内存快照（Base64 编码，如 state.json）
    pub memory_snapshot: Option<String>,
    /// 创建者
    pub created_by: AgentRole,
}

/// 单文件快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub content_hash: String,  // SHA256
    pub size: u64,
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

    /// 任务快照（Preparing 阶段捕获）
    #[serde(default)]
    pub snapshots: Vec<TaskSnapshot>,

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let now = Timestamp::now();
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
            metadata: TaskMetadata {
                created_at: Timestamp::now(),
                updated_at: Timestamp::now(),
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
        self.metadata.updated_at = Timestamp::now();
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

    /// 捕获当前快照（用于回滚）
    pub fn capture_snapshot(&mut self, files: Vec<FileSnapshot>) {
        self.snapshots.push(TaskSnapshot {
            id: SnapshotId::new(),
            captured_at: Timestamp::now(),
            git_commit: None,
            affected_files: files,
            memory_snapshot: None,
            created_by: self.metadata.created_by.clone(),
        });
    }

    /// 获取最新快照
    pub fn latest_snapshot(&self) -> Option<&TaskSnapshot> {
        self.snapshots.last()
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
pub type MemoryId = ulid::Ulid;
pub type WorkRecordId = ulid::Ulid;
pub type Timestamp = chrono::DateTime<chrono::Utc>;
