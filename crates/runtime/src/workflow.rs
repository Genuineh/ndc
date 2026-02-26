//! Workflow Engine - Simplified state machine with Saga pattern support
//!
//! Responsibilities:
//! - Manage task state transitions
//! - Execute transition rules
//! - Handle blocking and resumption
//! - Saga pattern for distributed transactions
//! - Compensating transactions for rollback

use ndc_core::{Executor, Task, TaskId, TaskState, WorkEvent, WorkRecord, WorkResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};

/// Workflow error
#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("Invalid state transition: {from:?} -> {to:?}")]
    InvalidTransition { from: TaskState, to: TaskState },
    #[error("Saga step {step} failed: {reason}")]
    SagaStepFailed { step: String, reason: String },
    #[error("Saga compensation failed: {reason}")]
    CompensationFailed { reason: String },
    #[error("Saga {saga_id} not found")]
    SagaNotFound { saga_id: String },
}

/// Workflow listener trait
#[async_trait::async_trait]
pub trait WorkflowListener: Send + Sync {
    async fn on_transition(&self, task_id: &TaskId, from: &TaskState, to: &TaskState);
}

/// Workflow transition rule
#[derive(Debug, Clone)]
pub struct TransitionRule {
    pub from: TaskState,
    pub to: TaskState,
    pub allowed: bool,
    pub auto_transition: bool,
}

/// Simplified workflow engine
#[derive(Default)]
pub struct WorkflowEngine {
    rules: HashMap<(TaskState, TaskState), TransitionRule>,
    listeners: Vec<Arc<dyn WorkflowListener>>,
}

impl std::fmt::Debug for WorkflowEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkflowEngine")
            .field("rules_count", &self.rules.len())
            .field("listeners_count", &self.listeners.len())
            .finish()
    }
}

impl WorkflowEngine {
    pub fn new() -> Self {
        let mut engine = Self::default();

        // Define common transition rules
        let rules = vec![
            // Normal flow
            (TaskState::Pending, TaskState::Preparing),
            (TaskState::Preparing, TaskState::InProgress),
            (TaskState::InProgress, TaskState::AwaitingVerification),
            (TaskState::AwaitingVerification, TaskState::Completed),
            // Failure handling
            (TaskState::InProgress, TaskState::Failed),
            // Rollback
            (TaskState::Failed, TaskState::Pending),
            (TaskState::Completed, TaskState::Pending),
        ];

        for (from, to) in rules {
            engine.rules.insert(
                (from.clone(), to.clone()),
                TransitionRule {
                    from: from.clone(),
                    to: to.clone(),
                    allowed: true,
                    auto_transition: false,
                },
            );
        }

        engine
    }

    /// Check if transition is allowed
    pub fn can_transition(&self, from: &TaskState, to: &TaskState) -> bool {
        self.rules
            .get(&(from.clone(), to.clone()))
            .map(|r| r.allowed)
            .unwrap_or(false)
    }

    /// Execute state transition
    pub async fn transition(&self, task: &mut Task, to: TaskState) -> Result<(), WorkflowError> {
        let from = task.state.clone();

        if !self.can_transition(&from, &to) {
            return Err(WorkflowError::InvalidTransition {
                from,
                to: to.clone(),
            });
        }

        // Create work record
        let record = WorkRecord {
            id: ulid::Ulid::new(),
            timestamp: chrono::Utc::now(),
            event: WorkEvent::StepCompleted,
            executor: Executor::System,
            result: WorkResult::Success,
        };
        task.metadata.work_records.push(record);

        // Update state
        task.state = to.clone();

        // Notify listeners
        for listener in &self.listeners {
            listener.on_transition(&task.id, &from, &to).await;
        }

        debug!("Task {:?} transitioned: {:?} -> {:?}", task.id, from, to);
        Ok(())
    }

    /// Register a listener
    pub fn register_listener(&mut self, listener: Arc<dyn WorkflowListener>) {
        self.listeners.push(listener);
    }
}

// ============================================================================
// Saga Pattern Implementation
// ============================================================================

/// Saga execution state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SagaState {
    /// Saga is pending execution
    Pending,
    /// Saga is currently executing
    Running,
    /// Saga completed successfully
    Completed,
    /// Saga failed, compensating
    Compensating,
    /// Saga compensation completed
    Compensated,
    /// Saga failed and cannot be compensated
    Failed,
}

/// A single step in a Saga
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaStep {
    /// Unique step identifier
    pub id: String,
    /// Step name/description
    pub name: String,
    /// Forward action (normal execution)
    pub forward_action: SagaAction,
    /// Compensating action (rollback)
    #[serde(default)]
    pub compensating_action: Option<SagaAction>,
    /// Step execution status
    #[serde(default)]
    pub status: SagaStepStatus,
    /// Output from forward action
    #[serde(default)]
    pub output: Option<serde_json::Value>,
    /// Timestamp when step started
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Timestamp when step completed
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Saga action definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaAction {
    /// Action type (tool name, function name, etc.)
    pub action_type: String,
    /// Action parameters
    #[serde(default)]
    pub parameters: serde_json::Value,
    /// Timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// Retry count
    #[serde(default)]
    pub retries: u32,
}

fn default_timeout() -> u64 {
    60
}

/// Saga step execution status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[derive(Default)]
pub enum SagaStepStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
    Compensating,
    Compensated,
}


/// Saga definition - a distributed transaction with compensation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saga {
    /// Unique saga identifier
    pub id: String,
    /// Saga name/description
    pub name: String,
    /// Associated task ID
    pub task_id: Option<String>,
    /// Saga steps in execution order
    pub steps: Vec<SagaStep>,
    /// Current execution state
    pub state: SagaState,
    /// Current step index
    pub current_step: Option<usize>,
    /// Error message if failed
    pub error: Option<String>,
    /// Created at
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Updated at
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Saga execution result
#[derive(Debug, Clone)]
pub struct SagaResult {
    pub saga_id: String,
    pub success: bool,
    pub completed_steps: usize,
    pub total_steps: usize,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub compensated: bool,
}

/// Saga orchestrator - manages Saga execution and compensation
pub struct SagaOrchestrator {
    /// Active sagas
    sagas: HashMap<String, Saga>,
    /// Saga storage for persistence
    storage: Option<Arc<dyn SagaStorage>>,
    /// Action executor
    executor: Arc<dyn SagaActionExecutor>,
}

/// Trait for persisting Saga state
#[async_trait::async_trait]
pub trait SagaStorage: Send + Sync {
    async fn save_saga(&self, saga: &Saga) -> Result<(), Box<dyn std::error::Error>>;
    async fn load_saga(&self, id: &str) -> Result<Option<Saga>, Box<dyn std::error::Error>>;
    async fn list_sagas(&self) -> Result<Vec<Saga>, Box<dyn std::error::Error>>;
    async fn delete_saga(&self, id: &str) -> Result<(), Box<dyn std::error::Error>>;
}

/// Trait for executing Saga actions
#[async_trait::async_trait]
pub trait SagaActionExecutor: Send + Sync {
    async fn execute_forward(&self, action: &SagaAction) -> Result<serde_json::Value, String>;
    async fn execute_compensate(&self, action: &SagaAction) -> Result<(), String>;
}

impl SagaOrchestrator {
    /// Create new Saga orchestrator
    pub fn new(executor: Arc<dyn SagaActionExecutor>) -> Self {
        Self {
            sagas: HashMap::new(),
            storage: None,
            executor,
        }
    }

    /// Set storage backend
    pub fn with_storage(mut self, storage: Arc<dyn SagaStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Create a new Saga
    pub fn create_saga(&mut self, name: String, task_id: Option<String>) -> Saga {
        
        Saga {
            id: ulid::Ulid::new().to_string(),
            name,
            task_id,
            steps: Vec::new(),
            state: SagaState::Pending,
            current_step: None,
            error: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// Add a step to a Saga
    pub fn add_step(&mut self, saga_id: &str, step: SagaStep) -> Result<(), WorkflowError> {
        let saga = self
            .sagas
            .get_mut(saga_id)
            .ok_or_else(|| WorkflowError::SagaNotFound {
                saga_id: saga_id.to_string(),
            })?;

        if saga.state != SagaState::Pending {
            return Err(WorkflowError::SagaStepFailed {
                step: saga_id.to_string(),
                reason: format!("Cannot add step to saga in {:?} state", saga.state),
            });
        }

        saga.steps.push(step);
        saga.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Register a Saga
    pub fn register_saga(&mut self, saga: Saga) {
        self.sagas.insert(saga.id.clone(), saga);
    }

    /// Execute a Saga
    pub async fn execute_saga(&mut self, saga_id: &str) -> Result<SagaResult, WorkflowError> {
        {
            let saga = self
                .sagas
                .get_mut(saga_id)
                .ok_or_else(|| WorkflowError::SagaNotFound {
                    saga_id: saga_id.to_string(),
                })?;

            // Update state
            saga.state = SagaState::Running;
            saga.updated_at = chrono::Utc::now();

            // Persist if storage available
            let saga_clone = saga.clone();
            if let Some(storage) = &self.storage {
                let _ = storage.save_saga(&saga_clone).await;
            }
        }

        let total_steps = {
            let saga = self.sagas.get(saga_id).unwrap();
            saga.steps.len()
        };

        let mut completed_steps = 0;
        let mut last_output = None;

        // Execute steps in order
        for idx in 0..total_steps {
            let step_name = {
                let saga = self.sagas.get(saga_id).unwrap();
                saga.steps.get(idx).map(|s| s.name.clone())
            };

            let step_name = step_name.unwrap_or_else(|| format!("step-{}", idx));

            info!(saga = %saga_id, step = %step_name, "Executing Saga step");

            // Get step action
            let forward_action = {
                let saga = self.sagas.get(saga_id).unwrap();
                saga.steps.get(idx).map(|s| s.forward_action.clone())
            };

            let forward_action = forward_action.ok_or_else(|| WorkflowError::SagaStepFailed {
                step: step_name.clone(),
                reason: "Step not found".to_string(),
            })?;

            // Execute forward action
            let result = self.executor.execute_forward(&forward_action).await;
            let mut step_error: Option<String> = None;

            {
                let saga = self.sagas.get_mut(saga_id).unwrap();
                let step = saga.steps.get_mut(idx).unwrap();

                saga.current_step = Some(idx);
                step.started_at = Some(chrono::Utc::now());

                match &result {
                    Ok(output) => {
                        step.status = SagaStepStatus::Completed;
                        step.completed_at = Some(chrono::Utc::now());
                        step.output = Some(output.clone());
                        last_output = Some(output.clone());
                        completed_steps += 1;
                    }
                    Err(e) => {
                        step.status = SagaStepStatus::Failed;
                        step.completed_at = Some(chrono::Utc::now());
                        saga.state = SagaState::Failed;
                        saga.error = Some(e.clone());
                        step_error = Some(e.clone());

                        error!(saga = %saga_id, step = %step_name, error = %e, "Saga step failed");
                    }
                }

                // Persist after each step
                let saga_clone = saga.clone();
                if let Some(storage) = &self.storage {
                    let _ = storage.save_saga(&saga_clone).await;
                }
            }

            if let Some(error_msg) = step_error {
                self.compensate_saga(saga_id).await?;

                return Ok(SagaResult {
                    saga_id: saga_id.to_string(),
                    success: false,
                    completed_steps,
                    total_steps,
                    output: None,
                    error: Some(error_msg),
                    compensated: true,
                });
            }
        }

        // All steps completed
        {
            let saga = self.sagas.get_mut(saga_id).unwrap();
            saga.state = SagaState::Completed;
            saga.current_step = None;
            saga.updated_at = chrono::Utc::now();

            let saga_clone = saga.clone();
            if let Some(storage) = &self.storage {
                let _ = storage.save_saga(&saga_clone).await;
            }
        }

        info!(saga = %saga_id, "Saga completed successfully");

        Ok(SagaResult {
            saga_id: saga_id.to_string(),
            success: true,
            completed_steps,
            total_steps,
            output: last_output,
            error: None,
            compensated: false,
        })
    }

    /// Compensate a failed Saga (run compensating actions in reverse)
    pub async fn compensate_saga(&mut self, saga_id: &str) -> Result<(), WorkflowError> {
        let last_completed = {
            let saga = self
                .sagas
                .get_mut(saga_id)
                .ok_or_else(|| WorkflowError::SagaNotFound {
                    saga_id: saga_id.to_string(),
                })?;

            saga.state = SagaState::Compensating;
            saga.updated_at = chrono::Utc::now();

            info!(saga = %saga_id, "Starting Saga compensation");

            // Find the last completed step
            saga.current_step.unwrap_or(0)
        };

        // Compensate in reverse order
        for idx in (0..last_completed).rev() {
            let compensating_action = {
                let saga = self.sagas.get_mut(saga_id).unwrap();
                let step = &mut saga.steps[idx];

                // Skip if no compensating action
                match &step.compensating_action {
                    Some(action) => {
                        step.status = SagaStepStatus::Compensating;
                        action.clone()
                    }
                    None => {
                        warn!(saga = %saga_id, step = %step.name, "No compensating action, skipping");
                        step.status = SagaStepStatus::Skipped;
                        continue;
                    }
                }
            };

            info!(saga = %saga_id, step_idx = idx, "Compensating Saga step");

            let result = self.executor.execute_compensate(&compensating_action).await;

            {
                let saga = self.sagas.get_mut(saga_id).unwrap();
                let step = &mut saga.steps[idx];

                match result {
                    Ok(()) => {
                        step.status = SagaStepStatus::Compensated;
                        info!(saga = %saga_id, step_idx = idx, "Compensation successful");
                    }
                    Err(e) => {
                        saga.state = SagaState::Failed;
                        saga.error = Some(format!("Compensation failed: {}", e));

                        error!(saga = %saga_id, step_idx = idx, error = %e, "Compensation failed");

                        return Err(WorkflowError::CompensationFailed {
                            reason: format!("Step {}: {}", idx, e),
                        });
                    }
                }

                // Persist after each compensation
                if let Some(storage) = &self.storage {
                    let _ = storage.save_saga(saga).await;
                }
            }
        }

        {
            let saga = self.sagas.get_mut(saga_id).unwrap();
            saga.state = SagaState::Compensated;
            saga.updated_at = chrono::Utc::now();

            if let Some(storage) = &self.storage {
                let _ = storage.save_saga(saga).await;
            }
        }

        info!(saga = %saga_id, "Saga compensation completed");

        Ok(())
    }

    /// Get a Saga
    pub fn get_saga(&self, id: &str) -> Option<&Saga> {
        self.sagas.get(id)
    }

    /// List all Sagas
    pub fn list_sagas(&self) -> Vec<&Saga> {
        self.sagas.values().collect()
    }

    /// Load Sagas from storage
    pub async fn load_from_storage(&mut self) -> Result<usize, Box<dyn std::error::Error>> {
        let storage = self
            .storage
            .as_ref()
            .ok_or_else(|| "No storage configured".to_string())?;

        let sagas = storage.list_sagas().await?;
        let count = sagas.len();

        for saga in sagas {
            self.sagas.insert(saga.id.clone(), saga);
        }

        info!(count, "Sagas loaded from storage");

        Ok(count)
    }
}

// ============================================================================
// In-Memory Saga Storage (default implementation)
// ============================================================================

/// In-memory Saga storage
pub struct MemorySagaStorage {
    sagas: std::sync::Arc<tokio::sync::RwLock<HashMap<String, Saga>>>,
}

impl MemorySagaStorage {
    pub fn new() -> Self {
        Self {
            sagas: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MemorySagaStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SagaStorage for MemorySagaStorage {
    async fn save_saga(&self, saga: &Saga) -> Result<(), Box<dyn std::error::Error>> {
        let mut sagas = self.sagas.write().await;
        sagas.insert(saga.id.clone(), saga.clone());
        Ok(())
    }

    async fn load_saga(&self, id: &str) -> Result<Option<Saga>, Box<dyn std::error::Error>> {
        let sagas = self.sagas.read().await;
        Ok(sagas.get(id).cloned())
    }

    async fn list_sagas(&self) -> Result<Vec<Saga>, Box<dyn std::error::Error>> {
        let sagas = self.sagas.read().await;
        Ok(sagas.values().cloned().collect())
    }

    async fn delete_saga(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut sagas = self.sagas.write().await;
        sagas.remove(id);
        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockExecutor;

    #[async_trait::async_trait]
    impl SagaActionExecutor for MockExecutor {
        async fn execute_forward(&self, action: &SagaAction) -> Result<serde_json::Value, String> {
            match action.action_type.as_str() {
                "failing" => Err("Action failed".to_string()),
                _ => Ok(serde_json::json!({"result": "success"})),
            }
        }

        async fn execute_compensate(&self, _action: &SagaAction) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn test_saga_state_default() {
        let state = SagaState::Pending;
        assert_eq!(state, SagaState::Pending);
    }

    #[test]
    fn test_saga_step_status_default() {
        let status = SagaStepStatus::default();
        assert_eq!(status, SagaStepStatus::Pending);
    }

    #[tokio::test]
    async fn test_create_saga() {
        let executor = Arc::new(MockExecutor);
        let mut orchestrator = SagaOrchestrator::new(executor);

        let saga = orchestrator.create_saga("test-saga".to_string(), None);

        assert_eq!(saga.name, "test-saga");
        assert_eq!(saga.state, SagaState::Pending);
        assert!(saga.task_id.is_none());
        assert!(saga.steps.is_empty());
    }

    #[tokio::test]
    async fn test_add_saga_step() {
        let executor = Arc::new(MockExecutor);
        let mut orchestrator = SagaOrchestrator::new(executor);

        let saga = orchestrator.create_saga("test-saga".to_string(), None);
        let saga_id = saga.id.clone();
        orchestrator.register_saga(saga);

        let step = SagaStep {
            id: "step-1".to_string(),
            name: "Test Step".to_string(),
            forward_action: SagaAction {
                action_type: "test".to_string(),
                parameters: serde_json::json!({}),
                timeout: 60,
                retries: 0,
            },
            compensating_action: None,
            status: SagaStepStatus::Pending,
            output: None,
            started_at: None,
            completed_at: None,
        };

        assert!(orchestrator.add_step(&saga_id, step).is_ok());
    }

    #[tokio::test]
    async fn test_execute_saga_success() {
        let executor = Arc::new(MockExecutor);
        let mut orchestrator = SagaOrchestrator::new(executor);

        let mut saga = orchestrator.create_saga("test-saga".to_string(), None);

        saga.steps.push(SagaStep {
            id: "step-1".to_string(),
            name: "Step 1".to_string(),
            forward_action: SagaAction {
                action_type: "test".to_string(),
                parameters: serde_json::json!({}),
                timeout: 60,
                retries: 0,
            },
            compensating_action: None,
            status: SagaStepStatus::Pending,
            output: None,
            started_at: None,
            completed_at: None,
        });

        let saga_id = saga.id.clone();
        orchestrator.register_saga(saga);

        let result = orchestrator.execute_saga(&saga_id).await.unwrap();

        assert!(result.success);
        assert_eq!(result.completed_steps, 1);
        assert_eq!(result.total_steps, 1);
    }

    #[tokio::test]
    async fn test_execute_saga_with_compensation() {
        let executor = Arc::new(MockExecutor);
        let mut orchestrator = SagaOrchestrator::new(executor);

        let mut saga = orchestrator.create_saga("test-saga".to_string(), None);

        saga.steps.push(SagaStep {
            id: "step-1".to_string(),
            name: "Step 1".to_string(),
            forward_action: SagaAction {
                action_type: "failing".to_string(),
                parameters: serde_json::json!({}),
                timeout: 60,
                retries: 0,
            },
            compensating_action: Some(SagaAction {
                action_type: "compensate".to_string(),
                parameters: serde_json::json!({}),
                timeout: 60,
                retries: 0,
            }),
            status: SagaStepStatus::Pending,
            output: None,
            started_at: None,
            completed_at: None,
        });

        let saga_id = saga.id.clone();
        orchestrator.register_saga(saga);

        let result = orchestrator.execute_saga(&saga_id).await.unwrap();

        assert!(!result.success);
        assert!(result.compensated);
        assert!(result.error.is_some());

        let saga = orchestrator.get_saga(&saga_id).unwrap();
        assert_eq!(saga.state, SagaState::Compensated);
    }
}
