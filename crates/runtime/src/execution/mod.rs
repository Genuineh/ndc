//! Saga Pattern - Task Rollback Plan
//!
//! Ensures clean rollback when execution fails midway.
//! Each subtask generates compensating actions for potential rollback.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Saga ID
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct SagaId(pub String);

impl Default for SagaId {
    fn default() -> Self {
        Self(format!("saga-{}", uuid::Uuid::new_v4()))
    }
}

impl std::fmt::Display for SagaId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Step ID
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepId(pub String);

impl Default for StepId {
    fn default() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for StepId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Saga Plan - Complete rollback plan for a task
#[derive(Debug, Clone)]
pub struct SagaPlan {
    /// Saga ID
    pub id: SagaId,

    /// Root task ID
    pub root_task_id: String,

    /// Steps with undo actions
    pub steps: Vec<SagaStep>,

    /// Compensating transactions
    pub compensations: Vec<CompensationAction>,
}

/// Step in the saga
#[derive(Debug, Clone)]
pub struct SagaStep {
    /// Step ID
    pub step_id: StepId,

    /// The action that was taken
    pub action: StepAction,

    /// How to undo this step
    pub undo_action: Option<UndoAction>,

    /// Step status
    pub status: StepStatus,
}

/// Action that was performed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepAction {
    /// Created a file
    CreateFile { path: PathBuf },

    /// Modified a file
    ModifyFile {
        path: PathBuf,
        backup: Option<String>,
    },

    /// Deleted a file
    DeleteFile {
        path: PathBuf,
        backup: Option<String>,
    },

    /// Ran a shell command
    RunCommand {
        command: String,
        working_dir: Option<PathBuf>,
    },

    /// Made a git commit
    GitCommit { commit_hash: String, branch: String },

    /// Created a git branch
    GitBranch { branch_name: String },

    /// Installed dependency
    AddDependency { name: String, version: String },

    /// Other action
    Other { description: String },
}

/// Undo action for compensation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UndoAction {
    /// Delete a file
    DeleteFile { path: PathBuf },

    /// Restore file from backup
    RestoreFile { path: PathBuf, backup: String },

    /// Run a shell command
    ShellCommand { command: String, args: Vec<String> },

    /// Git revert
    GitRevert { commit_hash: String },

    /// Undo dependency
    RemoveDependency { name: String },

    /// Custom compensation
    Custom {
        handler: String,
        params: serde_json::Value,
    },
}

/// Compensation action
#[derive(Debug, Clone)]
pub struct CompensationAction {
    /// Step being compensated
    pub step_id: StepId,

    /// The undo action
    pub undo: UndoAction,
}

/// Step status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Executing,
    Completed,
    Failed,
    RolledBack,
}

/// Rollback error
#[derive(Debug, thiserror::Error)]
pub enum RollbackError {
    #[error("Step not found: {0}")]
    StepNotFound(StepId),

    #[error("Undo action failed: {0}")]
    UndoFailed(String),

    #[error("File operation error: {0}")]
    FileError(String),

    #[error("Git error: {0}")]
    GitError(String),
}

impl SagaPlan {
    /// Create empty saga plan
    pub fn new(root_task_id: String) -> Self {
        Self {
            id: SagaId::default(),
            root_task_id,
            steps: Vec::new(),
            compensations: Vec::new(),
        }
    }

    /// Add a step with its undo action
    pub fn add_step(
        &mut self,
        step_id: StepId,
        action: StepAction,
        undo_action: Option<UndoAction>,
    ) {
        let step = SagaStep {
            step_id: step_id.clone(),
            action,
            undo_action: undo_action.clone(),
            status: StepStatus::Pending,
        };
        self.steps.push(step);

        if let Some(undo) = undo_action {
            self.compensations
                .push(CompensationAction { step_id, undo });
        }
    }

    /// Mark step as completed
    pub fn mark_completed(&mut self, step_id: &StepId) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.step_id == *step_id) {
            step.status = StepStatus::Completed;
        }
    }

    /// Mark step as failed
    pub fn mark_failed(&mut self, step_id: &StepId) {
        if let Some(step) = self.steps.iter_mut().find(|s| s.step_id == *step_id) {
            step.status = StepStatus::Failed;
        }
    }

    /// Execute rollback from a specific step
    pub async fn rollback<F, Fut>(
        &self,
        from_step: &StepId,
        executor: &F,
    ) -> Result<(), RollbackError>
    where
        F: Fn(UndoAction) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        // Find the starting index
        let start_idx = self
            .steps
            .iter()
            .position(|s| s.step_id == *from_step)
            .ok_or(RollbackError::StepNotFound(from_step.clone()))?;

        // Roll back in reverse order
        for step in self.steps[..=start_idx].iter().rev() {
            if step.status == StepStatus::Completed
                && let Some(ref undo) = step.undo_action {
                    executor(undo.clone())
                        .await
                        .map_err(RollbackError::UndoFailed)?;
                }
        }

        Ok(())
    }

    /// Get summary
    pub fn summary(&self) -> SagaSummary {
        SagaSummary {
            id: self.id.to_string(),
            root_task_id: self.root_task_id.clone(),
            total_steps: self.steps.len(),
            completed_steps: self
                .steps
                .iter()
                .filter(|s| s.status == StepStatus::Completed)
                .count(),
            rollback_count: self.compensations.len(),
        }
    }
}

/// Summary of a saga
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaSummary {
    pub id: String,
    pub root_task_id: String,
    pub total_steps: usize,
    pub completed_steps: usize,
    pub rollback_count: usize,
}

/// Helper to create undo from step action
impl UndoAction {
    /// Create appropriate undo for a file creation
    pub fn from_create_file(path: &PathBuf) -> Self {
        UndoAction::DeleteFile { path: path.clone() }
    }

    /// Create appropriate undo for file modification
    pub fn from_modify_file(path: &PathBuf, backup: &Option<String>) -> Self {
        match backup {
            Some(b) => UndoAction::RestoreFile {
                path: path.clone(),
                backup: b.clone(),
            },
            None => UndoAction::DeleteFile { path: path.clone() },
        }
    }

    /// Create appropriate undo for git commit
    pub fn from_git_commit(commit_hash: &str) -> Self {
        UndoAction::GitRevert {
            commit_hash: commit_hash.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_saga_plan_new() {
        let saga = SagaPlan::new("task-123".to_string());

        assert!(saga.id.0.starts_with("saga-"));
        assert_eq!(saga.root_task_id, "task-123");
        assert!(saga.steps.is_empty());
        assert!(saga.compensations.is_empty());
    }

    #[test]
    fn test_add_step() {
        let mut saga = SagaPlan::new("task-123".to_string());

        saga.add_step(
            StepId::default(),
            StepAction::CreateFile {
                path: PathBuf::from("new.rs"),
            },
            Some(UndoAction::DeleteFile {
                path: PathBuf::from("new.rs"),
            }),
        );

        assert_eq!(saga.steps.len(), 1);
        assert_eq!(saga.compensations.len(), 1);
    }

    #[test]
    fn test_mark_completed() {
        let mut saga = SagaPlan::new("task-123".to_string());
        let step_id = StepId::default();

        saga.add_step(
            step_id.clone(),
            StepAction::Other {
                description: "test".to_string(),
            },
            None,
        );
        saga.mark_completed(&step_id);

        assert_eq!(saga.steps[0].status, StepStatus::Completed);
    }

    #[test]
    fn test_summary() {
        let mut saga = SagaPlan::new("task-123".to_string());

        saga.add_step(
            StepId::default(),
            StepAction::CreateFile {
                path: PathBuf::from("a.rs"),
            },
            Some(UndoAction::DeleteFile {
                path: PathBuf::from("a.rs"),
            }),
        );
        saga.add_step(
            StepId::default(),
            StepAction::CreateFile {
                path: PathBuf::from("b.rs"),
            },
            Some(UndoAction::DeleteFile {
                path: PathBuf::from("b.rs"),
            }),
        );

        // Clone the step_id before mark_completed
        let first_step_id = saga.steps[0].step_id.clone();
        saga.mark_completed(&first_step_id);

        let summary = saga.summary();

        assert_eq!(summary.total_steps, 2);
        assert_eq!(summary.completed_steps, 1);
        assert_eq!(summary.rollback_count, 2);
    }

    #[test]
    fn test_undo_from_create() {
        let undo = UndoAction::from_create_file(&PathBuf::from("test.rs"));
        assert!(matches!(undo, UndoAction::DeleteFile { .. }));
    }

    #[test]
    fn test_undo_from_modify_with_backup() {
        let backup = Some("backup content".to_string());
        let undo = UndoAction::from_modify_file(&PathBuf::from("test.rs"), &backup);
        match undo {
            UndoAction::RestoreFile { path, backup: b } => {
                assert_eq!(path, PathBuf::from("test.rs"));
                assert_eq!(b, "backup content");
            }
            _ => panic!("Expected RestoreFile"),
        }
    }

    #[test]
    fn test_undo_from_git_commit() {
        let undo = UndoAction::from_git_commit("abc123");
        match undo {
            UndoAction::GitRevert { commit_hash } => {
                assert_eq!(commit_hash, "abc123");
            }
            _ => panic!("Expected GitRevert"),
        }
    }
}
