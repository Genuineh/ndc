//! Executor - Task execution engine
//!
//! Responsibilities:
//! - Execute tasks
//! - Coordinate tools and quality gates
//! - Manage task lifecycle

use ndc_core::{
    Task, TaskId, TaskState, Action,
    ExecutionStep, StepStatus, ActionResult,
    AgentRole,
};
use crate::{WorkflowEngine, SharedStorage, ToolManager, QualityGateRunner};
use std::sync::Arc;
use thiserror::Error;
use tracing::info;

/// Executor error
#[derive(Debug, Error, Clone)]
pub enum ExecutionError {
    #[error("Task not found: {0}")]
    TaskNotFound(TaskId),

    #[error("Invalid state transition: {from:?} -> {to:?}")]
    InvalidStateTransition { from: TaskState, to: TaskState },

    #[error("Tool execution failed: {0}")]
    ToolError(String),

    #[error("Quality check failed: {0}")]
    QualityCheckFailed(String),
}

/// Execution context
#[derive(Clone)]
pub struct ExecutionContext {
    pub storage: SharedStorage,
    pub workflow_engine: Arc<WorkflowEngine>,
    pub tools: Arc<ToolManager>,
    pub quality_runner: Arc<QualityGateRunner>,
    pub project_root: std::path::PathBuf,
    pub current_role: AgentRole,
}

impl std::fmt::Debug for ExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("project_root", &self.project_root)
            .field("current_role", &self.current_role)
            .finish()
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self {
            storage: crate::create_memory_storage(),
            workflow_engine: Arc::new(WorkflowEngine::new()),
            tools: Arc::new(ToolManager::new()),
            quality_runner: Arc::new(QualityGateRunner::new()),
            project_root: std::path::PathBuf::from("."),
            current_role: AgentRole::Historian,
        }
    }
}

/// Execution result
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
    pub tools_executed: u32,
    pub checks_passed: u32,
    pub checks_failed: u32,
}

/// Executor
#[derive(Debug)]
pub struct Executor {
    context: Arc<ExecutionContext>,
}

impl Executor {
    pub fn new(context: ExecutionContext) -> Self {
        Self {
            context: Arc::new(context),
        }
    }

    /// Get reference to execution context
    pub fn context(&self) -> &Arc<ExecutionContext> {
        &self.context
    }

    /// Create a new task
    pub async fn create_task(
        &self,
        title: String,
        description: String,
        created_by: AgentRole,
    ) -> Result<Task, ExecutionError> {
        let task = Task::new(title, description, created_by);

        // Save to storage
        self.context.storage.save_task(&task).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        info!("Task created: {:?}", task.id);
        Ok(task)
    }

    /// Execute a task
    pub async fn execute_task(&self, task_id: TaskId) -> Result<ExecutionResult, ExecutionError> {
        let start_time = std::time::Instant::now();

        // Get task from storage
        let mut task = self.context.storage.get_task(&task_id).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?
            .ok_or(ExecutionError::TaskNotFound(task_id))?;

        info!("Executing task: {:?} ({})", task_id, task.title);

        // Transition: Pending -> Preparing
        self.context.workflow_engine.transition(&mut task, TaskState::Preparing).await
            .map_err(|_e| ExecutionError::InvalidStateTransition {
                from: task.state.clone(),
                to: TaskState::Preparing,
            })?;

        // Transition: Preparing -> InProgress
        self.context.workflow_engine.transition(&mut task, TaskState::InProgress).await
            .map_err(|_e| ExecutionError::InvalidStateTransition {
                from: task.state.clone(),
                to: TaskState::InProgress,
            })?;

        // Execute actions
        if let Some(ref intent) = task.intent {
            self.execute_action(&mut task.clone(), &intent.proposed_action).await?;
        }

        // Transition: InProgress -> AwaitingVerification
        self.context.workflow_engine.transition(&mut task, TaskState::AwaitingVerification).await
            .map_err(|_e| ExecutionError::InvalidStateTransition {
                from: task.state.clone(),
                to: TaskState::AwaitingVerification,
            })?;

        // Run quality gate
        if let Some(ref _gate) = task.quality_gate {
            self.context.quality_runner.run(_gate).await
                .map_err(|e| ExecutionError::QualityCheckFailed(e.to_string()))?;
        }

        // Transition: AwaitingVerification -> Completed
        self.context.workflow_engine.transition(&mut task, TaskState::Completed).await
            .map_err(|_e| ExecutionError::InvalidStateTransition {
                from: task.state.clone(),
                to: TaskState::Completed,
            })?;

        // Save final state
        self.context.storage.save_task(&task).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(ExecutionResult {
            success: true,
            task_id,
            final_state: task.state,
            steps: task.steps.clone(),
            output: "Task completed".to_string(),
            error: None,
            metrics: ExecutionMetrics {
                total_duration_ms: duration_ms,
                tools_executed: task.steps.len() as u32,
                checks_passed: 1,
                checks_failed: 0,
            },
        })
    }

    /// Execute a single action
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
            executed_at: Some(chrono::Utc::now().into()),
        };

        task.steps.push(step.clone());

        // Execute action
        let result = match action {
            Action::ReadFile { path } => {
                self.execute_read_file(path).await
            }
            Action::WriteFile { path, content } => {
                self.execute_write_file(path, content).await
            }
            _ => {
                Ok(ActionResult {
                    success: true,
                    output: "Action not implemented".to_string(),
                    error: None,
                    ..Default::default()
                })
            }
        };

        // Update step result
        let result_val = result?;
        let step_idx = task.steps.iter().position(|s| s.step_id == step.step_id).unwrap();
        task.steps[step_idx].status = StepStatus::Completed;
        task.steps[step_idx].result = Some(result_val);

        Ok(())
    }

    /// Execute read file
    async fn execute_read_file(&self, path: &std::path::PathBuf) -> Result<ActionResult, ExecutionError> {
        let tool = self.context.tools.get("fs")
            .ok_or_else(|| ExecutionError::ToolError("FsTool not found".to_string()))?;

        let result = tool.execute(&serde_json::json!({
            "operation": "read",
            "path": path.to_string_lossy()
        })).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        Ok(ActionResult {
            success: result.success,
            output: result.output,
            error: result.error,
            ..Default::default()
        })
    }

    /// Execute write file
    async fn execute_write_file(&self, path: &std::path::PathBuf, content: &String) -> Result<ActionResult, ExecutionError> {
        let tool = self.context.tools.get("fs")
            .ok_or_else(|| ExecutionError::ToolError("FsTool not found".to_string()))?;

        let result = tool.execute(&serde_json::json!({
            "operation": "write",
            "path": path.to_string_lossy(),
            "content": content
        })).await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        Ok(ActionResult {
            success: result.success,
            output: result.output,
            error: result.error,
            ..Default::default()
        })
    }
}
