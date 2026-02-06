//! Intent and Verdict - 决策引擎核心类型
//!
//! - Intent: AI 提出的行动提案
//! - Verdict: 决策引擎的裁决结果（含权限等级）
//! - Effect: 声明的影响范围

use serde::{Deserialize, Serialize};
use crate::{AgentRole, AgentId, TaskId, TaskState, MemoryId, Timestamp, QualityCheckType};
use std::path::PathBuf;
use std::fmt;

/// Intent ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntentId(pub ulid::Ulid);

impl IntentId {
    pub fn new() -> Self {
        Self(ulid::Ulid::new())
    }
}

impl Default for IntentId {
    fn default() -> Self {
        Self::new()
    }
}

/// Intent - AI 的行动提案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    /// Intent ID
    pub id: IntentId,

    /// 发起者 ID
    pub agent: AgentId,

    /// 发起者角色
    pub agent_role: AgentRole,

    /// 提议的动作
    pub proposed_action: Action,

    /// 声明的影响范围
    pub effects: Vec<Effect>,

    /// 推理过程
    pub reasoning: String,

    /// 关联任务 ID
    pub task_id: Option<TaskId>,

    /// 创建时间
    pub timestamp: Timestamp,
}

/// Action - 提议的动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    /// 读文件
    ReadFile { path: PathBuf },

    /// 写文件
    WriteFile { path: PathBuf, content: String },

    /// 创建文件
    CreateFile { path: PathBuf },

    /// 删除文件
    DeleteFile { path: PathBuf },

    /// 执行命令
    RunCommand { command: String, args: Vec<String> },

    /// Git 操作
    Git { operation: GitOp },

    /// 修改内存
    ModifyMemory { memory_id: MemoryId, changes: String },

    /// 创建任务
    CreateTask { task_spec: TaskSpec },

    /// 更新任务状态
    UpdateTaskState { task_id: TaskId, new_state: TaskState },

    /// 搜索知识
    SearchKnowledge { query: String },

    /// 保存知识
    SaveKnowledge { knowledge: KnowledgeSpec },

    /// 运行测试
    RunTests { test_type: TestType },

    /// 质量检查（使用 task::QualityCheckType）
    RunQualityCheck { check_type: QualityCheckType },

    /// 请求人类介入
    RequestHuman { question: String, context: String },

    /// 其他动作
    Other { name: String, params: serde_json::Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GitOp {
    Status,
    Commit { message: String },
    Push,
    Pull,
    Branch { name: String },
    Checkout { branch: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestType {
    Unit,
    Integration,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub title: String,
    pub description: String,
    pub task_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSpec {
    pub title: String,
    pub content: String,
    pub knowledge_type: KnowledgeType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KnowledgeType {
    CodeSnippet,
    Documentation,
    Decision,
    Pattern,
    Tutorial,
}

/// Effect - 声明的影响范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Effect {
    /// 文件操作
    FileOperation { path: PathBuf, operation: FileOp },

    /// 任务状态转换
    TaskTransition { task_id: TaskId, from: TaskState, to: TaskState },

    /// 内存操作
    MemoryOperation { memory_id: MemoryId, operation: MemoryOp },

    /// 工具调用
    ToolInvocation { tool: String, args: Vec<String> },

    /// 人类交互
    HumanInteraction { interaction_type: InteractionType },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileOp {
    Read,
    Write,
    Create,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryOp {
    Read,
    Write,
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractionType {
    Approval,
    Decision,
    Information,
}

/// 权限等级 - 用于精细化控制
///
/// 示例：
/// - ReadFile(src/) → Normal 权限
/// - WriteFile(src/) → Normal 权限
/// - WriteFile(Cargo.toml) → Elevated 权限
/// - DeleteFile(any) → High 权限
/// - RunCommand(git reset --hard) → Critical 权限
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PrivilegeLevel {
    /// 普通权限 - 读文件、普通写操作
    Normal = 0,

    /// 提升权限 - 配置文件修改、构建操作
    Elevated = 1,

    /// 高权限 - 删除文件、强制操作
    High = 2,

    /// 关键权限 - 危险命令、系统修改
    Critical = 3,
}

impl fmt::Display for PrivilegeLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrivilegeLevel::Normal => write!(f, "Normal"),
            PrivilegeLevel::Elevated => write!(f, "Elevated"),
            PrivilegeLevel::High => write!(f, "High"),
            PrivilegeLevel::Critical => write!(f, "Critical"),
        }
    }
}

/// Verdict - 决策引擎的裁决结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Verdict {
    /// 允许执行（带权限等级）
    Allow {
        /// 动作
        action: Action,

        /// 授予的权限等级
        privilege: PrivilegeLevel,

        /// 附加条件
        conditions: Vec<Condition>,
    },

    /// 拒绝执行
    Deny {
        /// 原始动作
        action: Action,

        /// 拒绝原因
        reason: String,

        /// 错误码
        error_code: ErrorCode,
    },

    /// 需要人类介入
    RequireHuman {
        /// 原始动作
        action: Action,

        /// 询问问题
        question: String,

        /// 上下文
        context: HumanContext,

        /// 超时时间（秒）
        timeout: Option<u64>,
    },

    /// 修改后执行
    Modify {
        /// 原始动作
        original_action: Action,

        /// 修改后的动作
        modified_action: Action,

        /// 修改原因
        reason: String,

        /// 警告
        warnings: Vec<String>,
    },

    /// 延迟决策
    Defer {
        /// 原始动作
        action: Action,

        /// 需要的信息
        required_info: Vec<InformationRequirement>,

        /// 重试间隔（秒）
        retry_after: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub condition_type: ConditionType,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConditionType {
    /// 必须通过测试
    MustPassTests,

    /// 必须通过 lint
    MustPassLint,

    /// 必须审查
    MustReview,

    /// 必须文档化
    MustDocument,

    /// 需要特定权限
    RequirePrivilege(PrivilegeLevel),

    /// 自定义条件
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorCode {
    /// 未授权操作
    Unauthorized,

    /// 超出范围
    OutOfScope,

    /// 危险操作
    DangerousOperation,

    /// 无效动作
    InvalidAction,

    /// 依赖未满足
    DependencyNotMet,

    /// 权限不足
    InsufficientPrivilege {
        required: PrivilegeLevel,
        granted: PrivilegeLevel,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanContext {
    pub task_id: Option<TaskId>,
    pub affected_files: Vec<PathBuf>,
    pub risk_level: RiskLevel,
    pub alternatives: Vec<Action>,
    pub required_privilege: PrivilegeLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InformationRequirement {
    pub description: String,
    pub source: InformationSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InformationSource {
    Human,
    Memory,
    ExternalAPI,
}

// 类型别名
// 使用 ndc_core::AgentId 和 ndc_core::Timestamp (从 agent.rs 和 task.rs 重导出)
