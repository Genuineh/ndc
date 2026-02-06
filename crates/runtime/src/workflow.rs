//! Workflow Engine - Simplified state machine
//!
//! Responsibilities:
//! - Manage task state transitions
//! - Execute transition rules
//! - Handle blocking and resumption

use ndc_core::{Task, TaskState, WorkRecord, WorkEvent, Executor, WorkResult, TaskId};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tracing::debug;

/// Workflow error
#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("Invalid state transition: {from:?} -> {to:?}")]
    InvalidTransition { from: TaskState, to: TaskState },
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
        self.rules.get(&(from.clone(), to.clone()))
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
            timestamp: chrono::Utc::now().into(),
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
