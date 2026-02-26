//! Task Lineage - Parent-child task inheritance
//!
//! Tracks task relationships and inherits context from parent tasks.
//! Enables knowledge transfer across related tasks.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Failure pattern (redefined to avoid circular dependency)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePattern {
    pub error_type: String,
    pub message: String,
    pub file: Option<PathBuf>,
    pub root_cause: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Task Lineage - Records parent-child relationships and inherited context
#[derive(Debug, Clone)]
pub struct TaskLineage {
    /// Task ID this lineage belongs to
    pub task_id: String,

    /// Parent task ID (if this is a subtask)
    pub parent: Option<String>,

    /// Children task IDs (if this is a parent)
    pub children: Vec<String>,

    /// Inherited invariants from parent/failure history
    pub inherited_invariants: Vec<InheritedInvariant>,

    /// Inherited failure patterns
    pub inherited_failures: Vec<FailurePattern>,

    /// Inherited working memory context
    pub inherited_context: Option<ArchivedContext>,

    /// Generation depth (0 = root)
    pub depth: u32,

    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Inherited invariant with source tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritedInvariant {
    /// Source task ID where invariant was created
    pub source_task_id: String,

    /// The invariant rule
    pub rule: String,

    /// Why this invariant was created
    pub reason: String,

    /// How many times it has been validated
    pub validation_count: u32,

    /// Last validation timestamp
    pub last_validated: chrono::DateTime<chrono::Utc>,
}

/// Archived working memory from parent task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedContext {
    /// From which task
    pub source_task_id: String,

    /// Summary of what was accomplished
    pub accomplishment_summary: String,

    /// Key files modified
    pub key_files: Vec<PathBuf>,

    /// API surface used
    pub api_surface: Vec<String>,

    /// Critical decisions made
    pub decisions: Vec<String>,

    /// What failed and why
    pub failures: Vec<ArchivedFailure>,

    /// Archives at
    pub archived_at: chrono::DateTime<chrono::Utc>,
}

/// Archived failure for inheritance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedFailure {
    /// Error type
    pub error_type: String,

    /// Error message
    pub message: String,

    /// Root cause
    pub root_cause: String,

    /// How it was resolved
    pub resolution: String,

    /// Was it human-corrected?
    pub human_corrected: bool,
}

/// Lineage configuration
#[derive(Debug, Clone)]
pub struct LineageConfig {
    /// Enable lineage tracking
    pub enabled: bool,

    /// Max inheritance depth (0 = no limit)
    pub max_depth: u32,

    /// Inherit invariants
    pub inherit_invariants: bool,

    /// Inherit failures
    pub inherit_failures: bool,

    /// Inherit context
    pub inherit_context: bool,

    /// Min validation count for inheritance
    pub min_validation_count: u32,
}

impl Default for LineageConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_depth: 3,
            inherit_invariants: true,
            inherit_failures: true,
            inherit_context: true,
            min_validation_count: 2,
        }
    }
}

/// Lineage Service
#[derive(Debug, Clone)]
pub struct LineageService {
    /// Configuration
    config: LineageConfig,

    /// Lineage storage (in-memory for now)
    lineage_store: Vec<TaskLineage>,
}

impl LineageService {
    /// Create new lineage service
    pub fn new(config: Option<LineageConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
            lineage_store: Vec::new(),
        }
    }

    /// Create lineage for a new task (optionally from parent)
    pub fn create_lineage(
        &mut self,
        task_id: String,
        parent_task_id: Option<String>,
    ) -> Result<(), LineageError> {
        if !self.config.enabled {
            return Err(LineageError::Disabled);
        }

        let created_at = chrono::Utc::now();
        let depth = if let Some(ref parent) = parent_task_id {
            // Find parent depth
            let parent_depth = self
                .lineage_store
                .iter()
                .find(|l| &l.task_id == parent)
                .map(|l| l.depth)
                .unwrap_or(0);

            // Check max depth
            if self.config.max_depth > 0 && parent_depth + 1 > self.config.max_depth {
                return Err(LineageError::DepthExceeded(
                    parent_depth + 1,
                    self.config.max_depth,
                ));
            }

            // Add child to parent's children
            for lineage in &mut self.lineage_store {
                if &lineage.task_id == parent {
                    lineage.children.push(task_id.clone());
                    break;
                }
            }

            parent_depth + 1
        } else {
            0
        };

        let lineage = TaskLineage {
            task_id,
            parent: parent_task_id,
            children: Vec::new(),
            inherited_invariants: Vec::new(),
            inherited_failures: Vec::new(),
            inherited_context: None,
            depth,
            created_at,
        };

        self.lineage_store.push(lineage);
        Ok(())
    }

    /// Add inherited invariant
    pub fn add_inherited_invariant(&mut self, task_id: &str, invariant: InheritedInvariant) {
        if let Some(lineage) = self
            .lineage_store
            .iter_mut()
            .find(|l| l.task_id == task_id)
        {
            lineage.inherited_invariants.push(invariant);
        }
    }

    /// Get all inherited invariants for a task
    pub fn get_inherited_invariants(&self, task_id: &str) -> Vec<&InheritedInvariant> {
        self.lineage_store
            .iter()
            .find(|l| l.task_id == task_id)
            .map(|l| l.inherited_invariants.iter())
            .unwrap_or_default()
            .collect()
    }

    /// Archive context from completed task
    pub fn archive_context(&mut self, task_id: &str, context: ArchivedContext) {
        if let Some(lineage) = self
            .lineage_store
            .iter_mut()
            .find(|l| l.task_id == task_id)
        {
            lineage.inherited_context = Some(context);
        }
    }

    /// Get lineage for a task
    pub fn get_lineage(&self, task_id: &str) -> Option<&TaskLineage> {
        self.lineage_store.iter().find(|l| l.task_id == task_id)
    }

    /// Get summary
    pub fn summary(&self) -> LineageSummary {
        LineageSummary {
            total_lineages: self.lineage_store.len(),
            enabled: self.config.enabled,
            max_depth: self.config.max_depth,
        }
    }
}

/// Lineage errors
#[derive(Debug, thiserror::Error)]
pub enum LineageError {
    #[error("Lineage tracking is disabled")]
    Disabled,

    #[error("Depth exceeded: {0} (max: {1})")]
    DepthExceeded(u32, u32),
}

/// Lineage summary
#[derive(Debug, Clone)]
pub struct LineageSummary {
    pub total_lineages: usize,
    pub enabled: bool,
    pub max_depth: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_root_lineage() {
        let mut service = LineageService::new(None);
        let result = service.create_lineage("task-1".to_string(), None);

        assert!(result.is_ok());
        assert_eq!(service.summary().total_lineages, 1);
    }

    #[test]
    fn test_create_child_lineage() {
        let mut service = LineageService::new(None);
        service.create_lineage("parent".to_string(), None).unwrap();
        service
            .create_lineage("child".to_string(), Some("parent".to_string()))
            .unwrap();

        let parent = service.get_lineage("parent").unwrap();
        assert_eq!(parent.children, vec!["child"]);
        assert_eq!(parent.depth, 0);
    }

    #[test]
    fn test_add_inherited_invariant() {
        let mut service = LineageService::new(None);
        service.create_lineage("task-1".to_string(), None).unwrap();

        service.add_inherited_invariant(
            "task-1",
            InheritedInvariant {
                source_task_id: "old-task".to_string(),
                rule: "Always validate input".to_string(),
                reason: "Human correction".to_string(),
                validation_count: 5,
                last_validated: chrono::Utc::now(),
            },
        );

        let invariants = service.get_inherited_invariants("task-1");
        assert_eq!(invariants.len(), 1);
        assert_eq!(invariants[0].rule, "Always validate input");
    }

    #[test]
    fn test_archive_context() {
        let mut service = LineageService::new(None);
        service.create_lineage("task-1".to_string(), None).unwrap();

        service.archive_context(
            "task-1",
            ArchivedContext {
                source_task_id: "task-1".to_string(),
                accomplishment_summary: "Completed auth".to_string(),
                key_files: vec![PathBuf::from("auth.rs")],
                api_surface: vec!["authenticate".to_string()],
                decisions: vec!["Use JWT".to_string()],
                failures: Vec::new(),
                archived_at: chrono::Utc::now(),
            },
        );

        let lineage = service.get_lineage("task-1").unwrap();
        assert!(lineage.inherited_context.is_some());
    }

    #[test]
    fn test_max_depth_exceeded() {
        let mut service = LineageService::new(Some(LineageConfig {
            enabled: true,
            max_depth: 2,
            ..Default::default()
        }));

        service.create_lineage("task-0".to_string(), None).unwrap();
        service
            .create_lineage("task-1".to_string(), Some("task-0".to_string()))
            .unwrap();
        service
            .create_lineage("task-2".to_string(), Some("task-1".to_string()))
            .unwrap();

        let result = service.create_lineage("task-3".to_string(), Some("task-2".to_string()));

        assert!(matches!(result, Err(LineageError::DepthExceeded(3, 2))));
    }
}
