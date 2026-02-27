//! Executor - Task execution engine
//!
//! Responsibilities:
//! - Execute tasks
//! - Coordinate tools and quality gates
//! - Manage task lifecycle

use crate::discovery::DiscoveryService;
use crate::{HardConstraints, QualityGateRunner, SharedStorage, ToolManager, WorkflowEngine};
use ndc_core::{
    AccessControl, Action, ActionResult, AgentId, AgentRole, ExecutionStep, MemoryContent,
    MemoryEntry, MemoryId, MemoryMetadata, MemoryStability, StepStatus, SystemFactInput, Task,
    TaskId, TaskState,
};
use std::collections::HashSet;
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, warn};

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

    #[error("Discovery failed: {0}")]
    DiscoveryFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscoveryFailureMode {
    Degrade,
    Block,
}

impl DiscoveryFailureMode {
    fn from_str(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "block" | "fail" | "strict" => Self::Block,
            _ => Self::Degrade,
        }
    }
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
        let storage = crate::create_memory_storage();
        Self {
            storage: storage.clone(),
            workflow_engine: Arc::new(WorkflowEngine::new()),
            tools: Arc::new(crate::create_default_tool_manager_with_storage(storage)),
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
        self.context
            .storage
            .save_task(&task)
            .await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        info!("Task created: {:?}", task.id);
        Ok(task)
    }

    /// Execute a task
    pub async fn execute_task(&self, task_id: TaskId) -> Result<ExecutionResult, ExecutionError> {
        let start_time = std::time::Instant::now();

        // Get task from storage
        let mut task = self
            .context
            .storage
            .get_task(&task_id)
            .await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?
            .ok_or(ExecutionError::TaskNotFound(task_id))?;

        info!("Executing task: {:?} ({})", task_id, task.title);

        // Transition: Pending -> Preparing
        self.context
            .workflow_engine
            .transition(&mut task, TaskState::Preparing)
            .await
            .map_err(|_e| ExecutionError::InvalidStateTransition {
                from: task.state.clone(),
                to: TaskState::Preparing,
            })?;

        // Transition: Preparing -> InProgress
        self.context
            .workflow_engine
            .transition(&mut task, TaskState::InProgress)
            .await
            .map_err(|_e| ExecutionError::InvalidStateTransition {
                from: task.state.clone(),
                to: TaskState::InProgress,
            })?;

        // Execute actions
        if let Some(action) = task
            .intent
            .as_ref()
            .map(|intent| intent.proposed_action.clone())
        {
            self.execute_action(&mut task, &action).await?;
        }

        // Transition: InProgress -> AwaitingVerification
        self.context
            .workflow_engine
            .transition(&mut task, TaskState::AwaitingVerification)
            .await
            .map_err(|_e| ExecutionError::InvalidStateTransition {
                from: task.state.clone(),
                to: TaskState::AwaitingVerification,
            })?;

        // Discovery -> HardConstraints -> QualityGate enforced chain
        let hard_constraints = self.discover_hard_constraints(&task).await?;
        self.context
            .quality_runner
            .run_with_constraints(task.quality_gate.as_ref(), hard_constraints.as_ref())
            .await
            .map_err(|e| ExecutionError::QualityCheckFailed(e.to_string()))?;

        // Transition: AwaitingVerification -> Completed
        self.context
            .workflow_engine
            .transition(&mut task, TaskState::Completed)
            .await
            .map_err(|_e| ExecutionError::InvalidStateTransition {
                from: task.state.clone(),
                to: TaskState::Completed,
            })?;

        // Save final state
        self.context
            .storage
            .save_task(&task)
            .await
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
    async fn execute_action(&self, task: &mut Task, action: &Action) -> Result<(), ExecutionError> {
        let step = ExecutionStep {
            step_id: task.steps.len() as u64 + 1,
            action: action.clone(),
            status: StepStatus::InProgress,
            result: None,
            executed_at: Some(chrono::Utc::now()),
        };

        task.steps.push(step.clone());

        // Execute action
        let result = match action {
            Action::ReadFile { path } => self.execute_read_file(path).await,
            Action::WriteFile { path, content } => self.execute_write_file(path, content).await,
            _ => Ok(ActionResult {
                success: true,
                output: "Action not implemented".to_string(),
                error: None,
                ..Default::default()
            }),
        };

        // Update step result
        let result_val = result?;
        let step_idx = task
            .steps
            .iter()
            .position(|s| s.step_id == step.step_id)
            .expect("step must exist in task");
        task.steps[step_idx].status = StepStatus::Completed;
        task.steps[step_idx].result = Some(result_val);

        Ok(())
    }

    async fn discover_hard_constraints(
        &self,
        task: &Task,
    ) -> Result<Option<HardConstraints>, ExecutionError> {
        let affected_files = self.collect_affected_files(task);
        if affected_files.is_empty() {
            return Ok(None);
        }

        let discovery = DiscoveryService::new(self.context.project_root.clone(), None);
        let mode = Self::resolve_discovery_failure_mode();
        match discovery
            .discover(
                task.id.to_string(),
                task.description.clone(),
                affected_files,
            )
            .await
        {
            Ok(result) => {
                if let Some(ref constraints) = result.hard_constraints {
                    let summary = constraints.summary();
                    let total_constraints = summary.regression_test_count
                        + summary.api_symbol_count
                        + summary.high_volatility_count
                        + summary.coupling_warning_count
                        + summary.version_constraint_count
                        + summary.validation_count;
                    let signal = DiscoverySignal {
                        dedupe_key: format!("task:{}:discovery_hard_constraints", task.id),
                        rule: format!(
                            "Discovery generated {} hard constraints for task {}",
                            total_constraints, task.id
                        ),
                        description: "Execution must respect generated hard constraints"
                            .to_string(),
                        priority: ndc_core::InvariantPriority::High,
                        tags: vec![
                            "discovery".to_string(),
                            "hard_constraints".to_string(),
                            "execution".to_string(),
                        ],
                        evidence: vec![
                            format!("task_id={}", task.id),
                            format!("constraints_id={}", constraints.id),
                            format!(
                                "high_volatility_modules={}",
                                constraints.high_volatility_modules.len()
                            ),
                        ],
                    };
                    if let Err(err) = self.record_discovery_signal(&task.id, signal).await {
                        warn!(task_id = %task.id, error = %err, "Failed to persist discovery constraint signal");
                    }
                }
                Ok(result.hard_constraints)
            }
            Err(err) => {
                let message = format!("task {} discovery error: {}", task.id, err);
                let signal = DiscoverySignal {
                    dedupe_key: format!("task:{}:discovery_failed", task.id),
                    rule: format!("Discovery must succeed for task {}", task.id),
                    description: format!("Discovery failed during execution: {}", err),
                    priority: match mode {
                        DiscoveryFailureMode::Block => ndc_core::InvariantPriority::Critical,
                        DiscoveryFailureMode::Degrade => ndc_core::InvariantPriority::High,
                    },
                    tags: vec![
                        "discovery".to_string(),
                        "failure".to_string(),
                        format!("mode={:?}", mode).to_ascii_lowercase(),
                    ],
                    evidence: vec![
                        format!("task_id={}", task.id),
                        format!("mode={:?}", mode),
                        format!("error={}", err),
                    ],
                };
                if let Err(persist_err) = self.record_discovery_signal(&task.id, signal).await {
                    warn!(task_id = %task.id, error = %persist_err, "Failed to persist discovery failure signal");
                }
                match mode {
                    DiscoveryFailureMode::Degrade => {
                        warn!(task_id = %task.id, error = %err, "Discovery phase failed, degrading to no hard constraints");
                        Ok(None)
                    }
                    DiscoveryFailureMode::Block => Err(ExecutionError::DiscoveryFailed(message)),
                }
            }
        }
    }

    fn resolve_discovery_failure_mode() -> DiscoveryFailureMode {
        if let Ok(mode) = std::env::var("NDC_DISCOVERY_FAILURE_MODE") {
            return DiscoveryFailureMode::from_str(&mode);
        }

        let mut loader = ndc_core::NdcConfigLoader::new();
        if loader.load().is_ok()
            && let Some(runtime) = loader.config().runtime.as_ref()
        {
            return DiscoveryFailureMode::from_str(&runtime.discovery_failure_mode);
        }

        DiscoveryFailureMode::Degrade
    }

    pub fn gold_memory_entry_id() -> MemoryId {
        let uuid = uuid::Uuid::parse_str("00000000-0000-0000-0000-00000000a801")
            .expect("gold memory entry id must be valid uuid");
        MemoryId(uuid)
    }

    async fn record_discovery_signal(
        &self,
        task_id: &TaskId,
        signal: DiscoverySignal,
    ) -> Result<(), ExecutionError> {
        let entry_id = Self::gold_memory_entry_id();
        let (mut service, migrated_from_v1) = self.load_gold_memory_service(entry_id).await?;
        service.upsert_system_fact(SystemFactInput {
            dedupe_key: signal.dedupe_key,
            rule: signal.rule,
            description: signal.description,
            scope_pattern: task_id.to_string(),
            priority: signal.priority,
            tags: signal.tags,
            evidence: signal.evidence,
            source: "executor_discovery".to_string(),
        });
        self.persist_gold_memory_service(entry_id, task_id, &service, migrated_from_v1)
            .await?;
        Ok(())
    }

    async fn load_gold_memory_service(
        &self,
        entry_id: MemoryId,
    ) -> Result<(ndc_core::GoldMemoryService, bool), ExecutionError> {
        let Some(entry) = self
            .context
            .storage
            .get_memory(&entry_id)
            .await
            .map_err(ExecutionError::ToolError)?
        else {
            return Ok((ndc_core::GoldMemoryService::new(), false));
        };

        match entry.content {
            MemoryContent::General { text, metadata } if metadata == "gold_memory_service/v2" => {
                let payload: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|e| ExecutionError::ToolError(e.to_string()))?;
                let service_json = payload.get("service").cloned().ok_or_else(|| {
                    ExecutionError::ToolError("invalid gold memory v2 payload".to_string())
                })?;
                serde_json::from_value(service_json)
                    .map(|service| (service, false))
                    .map_err(|e| ExecutionError::ToolError(e.to_string()))
            }
            MemoryContent::General { text, metadata } if metadata == "gold_memory_service/v1" => {
                serde_json::from_str(&text)
                    .map(|service| (service, true))
                    .map_err(|e| ExecutionError::ToolError(e.to_string()))
            }
            _ => Ok((ndc_core::GoldMemoryService::new(), false)),
        }
    }

    async fn persist_gold_memory_service(
        &self,
        entry_id: MemoryId,
        task_id: &TaskId,
        service: &ndc_core::GoldMemoryService,
        migrated_from_v1: bool,
    ) -> Result<(), ExecutionError> {
        let wrapper = serde_json::json!({
            "version": 2,
            "service": service,
            "migration": if migrated_from_v1 {
                serde_json::json!({
                    "from_version": 1,
                    "migrated_at": chrono::Utc::now(),
                    "trigger_task_id": task_id.to_string(),
                    "trigger_source": "executor_discovery"
                })
            } else {
                serde_json::Value::Null
            },
        });
        let entry = MemoryEntry {
            id: entry_id,
            content: MemoryContent::General {
                text: serde_json::to_string(&wrapper)
                    .map_err(|e| ExecutionError::ToolError(e.to_string()))?,
                metadata: "gold_memory_service/v2".to_string(),
            },
            embedding: Vec::new(),
            relations: Vec::new(),
            metadata: MemoryMetadata {
                stability: MemoryStability::Canonical,
                created_at: chrono::Utc::now(),
                created_by: AgentId::system(),
                source_task: *task_id,
                version: 2,
                modified_at: Some(chrono::Utc::now()),
                tags: vec!["gold-memory".to_string(), "discovery".to_string()],
            },
            access_control: AccessControl::new(AgentId::system(), MemoryStability::Canonical),
        };
        self.context
            .storage
            .save_memory(&entry)
            .await
            .map_err(ExecutionError::ToolError)
    }

    fn collect_affected_files(&self, task: &Task) -> Vec<std::path::PathBuf> {
        let mut files = HashSet::new();

        if let Some(intent) = task.intent.as_ref() {
            for path in Self::action_paths(&intent.proposed_action) {
                files.insert(path);
            }
        }

        for step in &task.steps {
            for path in Self::action_paths(&step.action) {
                files.insert(path);
            }
        }

        files.into_iter().collect()
    }

    fn action_paths(action: &Action) -> Vec<std::path::PathBuf> {
        match action {
            Action::ReadFile { path }
            | Action::WriteFile { path, .. }
            | Action::CreateFile { path }
            | Action::DeleteFile { path } => vec![path.clone()],
            _ => Vec::new(),
        }
    }

    /// Execute read file
    async fn execute_read_file(
        &self,
        path: &std::path::PathBuf,
    ) -> Result<ActionResult, ExecutionError> {
        let tool = self
            .context
            .tools
            .get("fs")
            .ok_or_else(|| ExecutionError::ToolError("FsTool not found".to_string()))?;

        let result = tool
            .execute(&serde_json::json!({
                "operation": "read",
                "path": path.to_string_lossy(),
                "working_dir": self.context.project_root.to_string_lossy(),
            }))
            .await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        Ok(ActionResult {
            success: result.success,
            output: result.output,
            error: result.error,
            ..Default::default()
        })
    }

    /// Execute write file
    async fn execute_write_file(
        &self,
        path: &std::path::PathBuf,
        content: &String,
    ) -> Result<ActionResult, ExecutionError> {
        let tool = self
            .context
            .tools
            .get("fs")
            .ok_or_else(|| ExecutionError::ToolError("FsTool not found".to_string()))?;

        let result = tool
            .execute(&serde_json::json!({
                "operation": "write",
                "path": path.to_string_lossy(),
                "content": content,
                "working_dir": self.context.project_root.to_string_lossy(),
            }))
            .await
            .map_err(|e| ExecutionError::ToolError(e.to_string()))?;

        Ok(ActionResult {
            success: result.success,
            output: result.output,
            error: result.error,
            ..Default::default()
        })
    }
}

struct DiscoverySignal {
    dedupe_key: String,
    rule: String,
    description: String,
    priority: ndc_core::InvariantPriority,
    tags: Vec<String>,
    evidence: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("env lock poisoned")
    }

    #[test]
    fn test_discovery_failure_mode_parse() {
        assert_eq!(
            DiscoveryFailureMode::from_str("degrade"),
            DiscoveryFailureMode::Degrade
        );
        assert_eq!(
            DiscoveryFailureMode::from_str("block"),
            DiscoveryFailureMode::Block
        );
        assert_eq!(
            DiscoveryFailureMode::from_str("strict"),
            DiscoveryFailureMode::Block
        );
        assert_eq!(
            DiscoveryFailureMode::from_str("anything"),
            DiscoveryFailureMode::Degrade
        );
    }

    #[test]
    fn test_discovery_failure_mode_env_override() {
        unsafe {
            std::env::set_var("NDC_DISCOVERY_FAILURE_MODE", "block");
        }
        assert_eq!(
            Executor::resolve_discovery_failure_mode(),
            DiscoveryFailureMode::Block
        );
        unsafe {
            std::env::remove_var("NDC_DISCOVERY_FAILURE_MODE");
        }
    }

    #[tokio::test]
    async fn test_execute_task_persists_intent_action_step() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("NDC_DISCOVERY_FAILURE_MODE", "degrade");
        }

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("main.rs");
        std::fs::write(&file_path, "fn main() {}").unwrap();

        let mut context = ExecutionContext::default();
        context.project_root = temp_dir.path().to_path_buf();
        let executor = Executor::new(context);

        let task = executor
            .create_task(
                "intent step persistence".to_string(),
                "ensure executed steps are stored on real task".to_string(),
                AgentRole::Implementer,
            )
            .await
            .unwrap();

        let mut stored = executor
            .context()
            .storage
            .get_task(&task.id)
            .await
            .unwrap()
            .unwrap();
        stored.intent = Some(ndc_core::Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Implementer,
            proposed_action: Action::ReadFile {
                path: file_path.clone(),
            },
            effects: Vec::new(),
            reasoning: "read source".to_string(),
            task_id: Some(task.id),
            timestamp: chrono::Utc::now(),
        });
        executor.context().storage.save_task(&stored).await.unwrap();

        let result = executor.execute_task(task.id).await.unwrap();
        assert!(result.success);
        assert!(
            result
                .steps
                .iter()
                .any(|step| matches!(step.action, Action::ReadFile { .. }))
        );
        assert!(!result.steps.is_empty());

        unsafe {
            std::env::remove_var("NDC_DISCOVERY_FAILURE_MODE");
        }
    }
}
