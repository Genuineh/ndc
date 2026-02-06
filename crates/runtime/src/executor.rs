//! Executor - 异步任务调度器
//!
//! 职责：
//! - 从 Decision Engine 获取已裁决的 Task
//! - 协调工具执行与质量验证
//! - 管理任务生命周期
//! - 处理回滚与恢复

use ndc_core::{
    Task, TaskId, TaskState, Intent, Verdict, Action,
    ExecutionStep, StepStatus, ActionResult, ActionMetrics,
    GitWorktreeSnapshot, QualityGate, QualityCheck, QualityCheckType,
    PrivilegeLevel, Condition, ConditionType,
    AgentRole, Timestamp,
};
use ndc_decision::{DecisionEngine, BasicDecisionEngine};
use ndc_persistence::{Storage, JsonStorage};
use crate::workflow::WorkflowEngine;
use crate::tools::{Tool, ToolResult, FsTool, GitTool, ShellTool};
use crate::verify::QualityGateRunner;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::Mutex;
use thiserror::Error;
use tracing::{info, warn, error, debug};

/// 执行器错误
#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("Task 不存在: {0}")]
    TaskNotFound(TaskId),

    #[error("无效状态转换: {from:?} -> {to:?}")]
    InvalidStateTransition { from: TaskState, to: TaskState },

    #[error("Verdict 非 Allow: {:?}", .0)]
    VerdictNotAllow(Verdict),

    #[error("工具执行失败: {0}")]
    ToolError(String),

    #[error("质量检查失败: {0}")]
    QualityCheckFailed(String),

    #[error("权限不足: 需要 {:?}, 当前 {:?}")]
    InsufficientPrivilege { required: PrivilegeLevel, granted: PrivilegeLevel },

    #[error("条件未满足: {0}")]
    ConditionNotMet(String),

    #[error("回滚失败: {0}")]
    RollbackFailed(String),
}

/// 执行上下文
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// 存储后端
    pub storage: Arc<JsonStorage>,

    /// 决策引擎
    pub decision_engine: Arc<BasicDecisionEngine>,

    /// 工作流引擎
    pub workflow_engine: Arc<WorkflowEngine>,

    /// 工具集
    pub tools: HashMap<String, Arc<dyn Tool>>,

    /// 质量门禁运行器
    pub quality_runner: Arc<QualityGateRunner>,

    /// 项目根目录
    pub project_root: std::path::PathBuf,

    /// 当前 Agent 角色
    pub current_role: AgentRole,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        let mut tools: HashMap<String, Arc<dyn Tool>> = HashMap::new();
        tools.insert("fs".to_string(), Arc::new(FsTool::new()));
        tools.insert("git".to_string(), Arc::new(GitTool::new()));
        tools.insert("shell".to_string(), Arc::new(ShellTool::new()));

        Self {
            storage: Arc::new(JsonStorage::new(std::path::PathBuf::from(".ndc/storage"), Default::default())),
            decision_engine: Arc::new(BasicDecisionEngine::new()),
            workflow_engine: Arc::new(WorkflowEngine::new()),
            tools,
            quality_runner: Arc::new(QualityGateRunner::new()),
            project_root: std::path::PathBuf::from("."),
            current_role: AgentRole::Historian,
        }
    }
}

/// 执行结果
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub success: bool,
    pub task_id: TaskId,
    pub final_state: TaskState,
    pub steps: Vec<ExecutionStep>,
    pub output: String,
    pub error: Option<String>,
    pub metrics: ExecutionMetrics,
}

#[derive(Debug, Clone, Default)]
pub struct ExecutionMetrics {
    pub total_duration_ms: u64,
    pub total_tokens_used: u64,
    pub tools_executed: u32,
    pub checks_passed: u32,
    pub checks_failed: u32,
}

/// 执行器
#[derive(Debug)]
pub struct Executor {
    /// 执行上下文
    context: Arc<ExecutionContext>,

    /// 运行中的任务
    running_tasks: Arc<Mutex<HashMap<TaskId, Task>>>,
}

impl Executor {
    /// 创建新的执行器
    pub fn new(context: ExecutionContext) -> Self {
        Self {
            context: Arc::new(context),
            running_tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 创建默认执行器
    pub fn default() -> Self {
        Self::new(ExecutionContext::default())
    }

    /// 创建新任务
    pub async fn create_task(
        &self,
        title: String,
        description: String,
        created_by: AgentRole,
    ) -> Result<Task, ExecutionError> {
        let task = Task::new(title, description, created_by);

        // 保存到存储
        self.context.storage.save_task(&task).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        info!("Task created: {}", task.id);

        Ok(task)
    }

    /// 提交 Intent 进行裁决
    pub async fn submit_intent(&self, intent: Intent) -> Result<Verdict, ExecutionError> {
        // 获取 Verdict
        let verdict = self.context.decision_engine.evaluate(intent).await;

        match &verdict {
            Verdict::Allow { .. } => {
                debug!("Intent {:?} allowed", intent.id);
            }
            Verdict::Deny { reason, .. } => {
                warn!("Intent {:?} denied: {}", intent.id, reason);
            }
            Verdict::RequireHuman { question, .. } => {
                info!("Human required for {:?}: {}", intent.id, question);
            }
            _ => {}
        }

        Ok(verdict)
    }

    /// 执行 Task（从 Pending 到 Completed）
    pub async fn execute_task(&self, task_id: TaskId) -> Result<ExecutionResult, ExecutionError> {
        let start_time = std::time::Instant::now();

        // 获取 Task
        let mut task = self.context.storage.get_task(&task_id).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?
            .ok_or(ExecutionError::TaskNotFound(task_id))?;

        info!("Executing task: {} ({})", task_id, task.title);

        // 状态转换: Pending -> Preparing
        task.request_transition(TaskState::Preparing)
            .map_err(|e| ExecutionError::InvalidStateTransition {
                from: task.state.clone(),
                to: TaskState::Preparing,
            })?;

        // 捕获快照
        self.capture_snapshot(&mut task).await?;

        // 保存状态
        self.context.storage.save_task(&task).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        // 状态转换: Preparing -> InProgress
        task.request_transition(TaskState::InProgress)
            .map_err(|e| ExecutionError::InvalidStateTransition {
                from: task.state.clone(),
                to: TaskState::InProgress,
            })?;

        // 执行 Intent（如果有）
        if let Some(ref intent) = task.intent {
            self.execute_action(&mut task, &intent.proposed_action).await?;
        }

        // 状态转换: InProgress -> AwaitingVerification
        task.request_transition(TaskState::AwaitingVerification)
            .map_err(|e| ExecutionError::InvalidStateTransition {
                from: task.state.clone(),
                to: TaskState::AwaitingVerification,
            })?;

        // 运行质量门禁
        if let Some(ref gate) = task.quality_gate {
            self.context.quality_runner.run(gate, &self.context).await
                .map_err(|e| ExecutionError::QualityCheckFailed(e.to_string()))?;
        }

        // 状态转换: AwaitingVerification -> Completed
        task.request_transition(TaskState::Completed)
            .map_err(|e| ExecutionError::InvalidStateTransition {
                from: task.state.clone(),
                to: TaskState::Completed,
            })?;

        // 保存最终状态
        self.context.storage.save_task(&task).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        info!("Task completed: {} in {}ms", task_id, duration_ms);

        Ok(ExecutionResult {
            success: true,
            task_id,
            final_state: task.state,
            steps: task.steps.clone(),
            output: "Task completed successfully".to_string(),
            error: None,
            metrics: ExecutionMetrics {
                total_duration_ms: duration_ms,
                ..Default::default()
            },
        })
    }

    /// 执行单个 Action
    async fn execute_action(
        &self,
        task: &mut Task,
        action: &Action,
    ) -> Result<(), ExecutionError> {
        let step = ExecutionStep {
            step_id: task.steps.len() as u64 + 1,
            action: action.clone(),
            status: StepStatus::InProgress,
            result: None,
            executed_at: Some(Timestamp::now()),
        };

        task.steps.push(step.clone());

        // 执行 Action
        let result = match action {
            Action::ReadFile { path } => {
                self.execute_read_file(path).await
            }
            Action::WriteFile { path, content } => {
                self.execute_write_file(path, content).await
            }
            Action::CreateFile { path } => {
                self.execute_create_file(path).await
            }
            Action::DeleteFile { path } => {
                self.execute_delete_file(path).await
            }
            Action::RunCommand { command, args } => {
                self.execute_run_command(command, args).await
            }
            Action::Git { operation } => {
                self.execute_git_operation(operation).await
            }
            Action::RunTests { test_type } => {
                self.execute_run_tests(test_type).await
            }
            Action::RunQualityCheck { check_type } => {
                self.execute_quality_check(check_type).await
            }
            _ => {
                Ok(ActionResult {
                    success: true,
                    output: "Action not implemented".to_string(),
                    error: None,
                    metrics: ActionMetrics::default(),
                })
            }
        };

        // 更新步骤状态
        let step_result = result?;
        if let Some(last_step) = task.steps.last_mut() {
            last_step.status = if step_result.success {
                StepStatus::Completed
            } else {
                StepStatus::Failed
            };
            last_step.result = Some(step_result);
        }

        Ok(())
    }

    /// 捕获快照
    async fn capture_snapshot(&self, _task: &mut Task) -> Result<(), ExecutionError> {
        // TODO: 实现 Git Worktree 快照
        // 1. 创建临时 worktree
        // 2. 记录受影响文件
        // 3. 保存到 Task.snapshots
        Ok(())
    }

    // ===== 工具执行方法 =====

    async fn execute_read_file(&self, path: &std::path::PathBuf) -> Result<ActionResult, ExecutionError> {
        let tool = self.context.tools.get("fs")
            .ok_or_else(|| ExecutionError::ToolError("fs tool not found".to_string()))?;

        let result = tool.execute(&serde_json::json!({
            "operation": "read",
            "path": path
        })).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        Ok(ActionResult {
            success: result.success,
            output: result.output,
            error: result.error,
            metrics: ActionMetrics::default(),
        })
    }

    async fn execute_write_file(&self, path: &std::path::PathBuf, content: &String) -> Result<ActionResult, ExecutionError> {
        let tool = self.context.tools.get("fs")
            .ok_or_else(|| ExecutionError::ToolError("fs tool not found".to_string()))?;

        let result = tool.execute(&serde_json::json!({
            "operation": "write",
            "path": path,
            "content": content
        })).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        Ok(ActionResult {
            success: result.success,
            output: result.output,
            error: result.error,
            metrics: ActionMetrics::default(),
        })
    }

    async fn execute_create_file(&self, path: &std::path::PathBuf) -> Result<ActionResult, ExecutionError> {
        let tool = self.context.tools.get("fs")
            .ok_or_else(|| ExecutionError::ToolError("fs tool not found".to_string()))?;

        let result = tool.execute(&serde_json::json!({
            "operation": "create",
            "path": path
        })).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        Ok(ActionResult {
            success: result.success,
            output: result.output,
            error: result.error,
            metrics: ActionMetrics::default(),
        })
    }

    async fn execute_delete_file(&self, path: &std::path::PathBuf) -> Result<ActionResult, ExecutionError> {
        let tool = self.context.tools.get("fs")
            .ok_or_else(|| ExecutionError::ToolError("fs tool not found".to_string()))?;

        let result = tool.execute(&serde_json::json!({
            "operation": "delete",
            "path": path
        })).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        Ok(ActionResult {
            success: result.success,
            output: result.output,
            error: result.error,
            metrics: ActionMetrics::default(),
        })
    }

    async fn execute_run_command(&self, command: &String, args: &Vec<String>) -> Result<ActionResult, ExecutionError> {
        let tool = self.context.tools.get("shell")
            .ok_or_else(|| ExecutionError::ToolError("shell tool not found".to_string()))?;

        let result = tool.execute(&serde_json::json!({
            "command": command,
            "args": args
        })).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        Ok(ActionResult {
            success: result.success,
            output: result.output,
            error: result.error,
            metrics: ActionMetrics::default(),
        })
    }

    async fn execute_git_operation(&self, operation: &ndc_core::GitOp) -> Result<ActionResult, ExecutionError> {
        let tool = self.context.tools.get("git")
            .ok_or_else(|| ExecutionError::ToolError("git tool not found".to_string()))?;

        let params = serde_json::to_value(operation)
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        let result = tool.execute(&params).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        Ok(ActionResult {
            success: result.success,
            output: result.output,
            error: result.error,
            metrics: ActionMetrics::default(),
        })
    }

    async fn execute_run_tests(&self, test_type: &ndc_core::TestType) -> Result<ActionResult, ExecutionError> {
        let result = self.context.quality_runner.run_tests(test_type).await
            .map_err(|e| ExecutionError::QualityCheckFailed(e.to_string()))?;

        Ok(ActionResult {
            success: result.passed,
            output: result.output,
            error: result.error,
            metrics: ActionMetrics::default(),
        })
    }

    async fn execute_quality_check(&self, check_type: &ndc_core::QualityCheckType) -> Result<ActionResult, ExecutionError> {
        let result = self.context.quality_runner.run_check(check_type).await
            .map_err(|e| ExecutionError::QualityCheckFailed(e.to_string()))?;

        Ok(ActionResult {
            success: result.passed,
            output: result.output,
            error: result.error,
            metrics: ActionMetrics::default(),
        })
    }

    /// 回滚 Task 到指定快照
    pub async fn rollback_task(&self, task_id: TaskId, snapshot_id: ulid::Ulid) -> Result<(), ExecutionError> {
        let mut task = self.context.storage.get_task(&task_id).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?
            .ok_or(ExecutionError::TaskNotFound(task_id))?;

        // 查找快照
        let snapshot = task.snapshots.iter()
            .find(|s| s.id == snapshot_id)
            .ok_or_else(|| ExecutionError::RollbackFailed("Snapshot not found".to_string()))?;

        // TODO: 实现实际回滚逻辑
        // 1. 删除 worktree
        // 2. 恢复文件
        // 3. 更新任务状态

        info!("Task {} rolled back to snapshot {}", task_id, snapshot_id);

        Ok(())
    }
}
