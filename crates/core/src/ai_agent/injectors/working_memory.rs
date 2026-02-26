//! Working Memory Injector
//!
//! Injects current working memory context into Agent prompts
//!
//! Design:
//! - Extract relevant context from WorkingMemory
//! - Format as readable text for the LLM
//! - Inject only relevant information based on task

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Working Memory context for agent injection
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct WorkingMemoryContext {
    /// Abstract layer summary (historical failures/root cause)
    pub abstract_summary: Option<String>,

    /// Raw layer summary (current step facts)
    pub raw_summary: Option<String>,

    /// Hard layer constraints (must-not-violate invariants)
    pub hard_constraints: Vec<String>,

    /// Current active files
    pub active_files: Vec<String>,

    /// API surface (relevant functions/classes)
    pub api_surface: Vec<String>,

    /// Recent failures to avoid
    pub recent_failures: Vec<String>,

    /// Current task context
    pub current_task: Option<TaskContext>,

    /// Custom context data
    pub custom: HashMap<String, Value>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub task_id: String,
    pub task_title: String,
    pub current_step: String,
    pub completed_steps: Vec<String>,
}

/// Working Memory Injector configuration
#[derive(Debug, Clone)]
pub struct WorkingMemoryInjectorConfig {
    /// Maximum number of active files to include
    pub max_active_files: usize,

    /// Maximum number of API entries to include
    pub max_api_entries: usize,

    /// Maximum number of recent failures to include
    pub max_failures: usize,

    /// Include task context
    pub include_task_context: bool,

    /// Include custom data
    pub include_custom: bool,
}

impl Default for WorkingMemoryInjectorConfig {
    fn default() -> Self {
        Self {
            max_active_files: 10,
            max_api_entries: 20,
            max_failures: 5,
            include_task_context: true,
            include_custom: false,
        }
    }
}

/// Working Memory Injector
#[derive(Debug, Clone)]
pub struct WorkingMemoryInjector {
    /// Current working memory
    memory: WorkingMemoryContext,

    /// Configuration
    config: WorkingMemoryInjectorConfig,
}

impl WorkingMemoryInjector {
    /// Create new injector
    pub fn new(config: WorkingMemoryInjectorConfig) -> Self {
        Self {
            memory: WorkingMemoryContext::default(),
            config,
        }
    }

    /// Create with default config
    pub fn with_default_config() -> Self {
        Self::new(WorkingMemoryInjectorConfig::default())
    }

    /// Build context from hierarchical WorkingMemory model (Abstract + Raw + Hard)
    pub fn from_working_memory(memory: &crate::WorkingMemory) -> WorkingMemoryContext {
        let abstract_summary = Some(format!(
            "Attempts: {}, Trajectory: {:?}, Root cause: {}",
            memory.abstract_history.attempt_count,
            memory.abstract_history.trajectory_state,
            memory
                .abstract_history
                .root_cause_summary
                .clone()
                .unwrap_or_else(|| "N/A".to_string())
        ));

        let raw_summary = memory
            .raw_current
            .current_step_context
            .as_ref()
            .map(|step| {
                format!(
                    "Step {} - {}{}",
                    step.step_index,
                    step.description,
                    step.expected_output
                        .as_ref()
                        .map(|v| format!(" | expected: {}", v))
                        .unwrap_or_default()
                )
            });

        let active_files = memory
            .raw_current
            .active_files
            .iter()
            .map(|p| p.display().to_string())
            .collect();

        let api_surface = memory
            .raw_current
            .api_surface
            .iter()
            .map(|api| {
                format!(
                    "{}::{:?} @ {}:{}",
                    api.name,
                    api.kind,
                    api.file.display(),
                    api.line
                )
            })
            .collect();

        let recent_failures = memory
            .abstract_history
            .failure_patterns
            .iter()
            .rev()
            .take(5)
            .map(|f| format!("{}: {}", f.error_type, f.message))
            .collect();

        let hard_constraints = memory
            .hard_invariants
            .iter()
            .map(|inv| format!("[{:?}] {}", inv.priority, inv.rule))
            .collect();

        WorkingMemoryContext {
            abstract_summary,
            raw_summary,
            hard_constraints,
            active_files,
            api_surface,
            recent_failures,
            current_task: None,
            custom: HashMap::new(),
        }
    }

    /// Update working memory
    pub fn update(&mut self, memory: WorkingMemoryContext) {
        self.memory = memory;
    }

    /// Set active files
    pub fn set_active_files(&mut self, files: Vec<String>) {
        self.memory.active_files = files
            .into_iter()
            .take(self.config.max_active_files)
            .collect();
    }

    /// Set API surface
    pub fn set_api_surface(&mut self, apis: Vec<String>) {
        self.memory.api_surface = apis.into_iter().take(self.config.max_api_entries).collect();
    }

    /// Add recent failure
    pub fn add_failure(&mut self, failure: String) {
        if self.memory.recent_failures.len() < self.config.max_failures {
            self.memory.recent_failures.push(failure);
        }
    }

    /// Clear failures
    pub fn clear_failures(&mut self) {
        self.memory.recent_failures.clear();
    }

    /// Set task context
    pub fn set_task_context(&mut self, task: TaskContext) {
        self.memory.current_task = Some(task);
    }

    /// Inject as prompt text
    pub fn inject(&self) -> String {
        let mut lines = Vec::new();

        lines.push("=== WORKING MEMORY ===".to_string());

        // Abstract layer
        if let Some(ref abstract_summary) = self.memory.abstract_summary {
            lines.push("\nAbstract Context:".to_string());
            lines.push(format!("  {}", abstract_summary));
        }

        // Raw layer
        if let Some(ref raw_summary) = self.memory.raw_summary {
            lines.push("\nRaw Context:".to_string());
            lines.push(format!("  {}", raw_summary));
        }

        // Hard layer
        if !self.memory.hard_constraints.is_empty() {
            lines.push("\nHard Constraints (MUST):".to_string());
            for constraint in &self.memory.hard_constraints {
                lines.push(format!("  - {}", constraint));
            }
        }

        // Active files
        if !self.memory.active_files.is_empty() {
            lines.push("\nActive Files:".to_string());
            for file in &self.memory.active_files {
                lines.push(format!("  - {}", file));
            }
        }

        // API surface
        if !self.memory.api_surface.is_empty() {
            lines.push("\nRelevant APIs:".to_string());
            for api in &self.memory.api_surface {
                lines.push(format!("  - {}", api));
            }
        }

        // Recent failures
        if !self.memory.recent_failures.is_empty() {
            lines.push("\nRecent Failures (avoid these patterns):".to_string());
            for failure in &self.memory.recent_failures {
                lines.push(format!("  - {}", failure));
            }
        }

        // Task context
        if self.config.include_task_context
            && let Some(ref task) = self.memory.current_task {
                lines.push("\nCurrent Task:".to_string());
                lines.push(format!("  ID: {}", task.task_id));
                lines.push(format!("  Title: {}", task.task_title));
                lines.push(format!("  Current Step: {}", task.current_step));
                if !task.completed_steps.is_empty() {
                    lines.push("\nCompleted Steps:".to_string());
                    for step in &task.completed_steps {
                        lines.push(format!("  - {}", step));
                    }
                }
            }

        // Custom data
        if self.config.include_custom && !self.memory.custom.is_empty() {
            lines.push("\nCustom Context:".to_string());
            for (key, value) in &self.memory.custom {
                lines.push(format!("  {}: {}", key, value));
            }
        }

        lines.push("\n=== END WORKING MEMORY ===".to_string());

        lines.join("\n")
    }

    /// Inject as structured data (for JSON-based prompts)
    pub fn inject_json(&self) -> Value {
        json!({
            "type": "working_memory",
            "active_files": self.memory.active_files,
            "api_surface": self.memory.api_surface,
            "recent_failures": self.memory.recent_failures,
            "task_context": self.memory.current_task,
        })
    }

    /// Check if there's relevant context
    pub fn has_context(&self) -> bool {
        self.memory.abstract_summary.is_some()
            || self.memory.raw_summary.is_some()
            || !self.memory.hard_constraints.is_empty()
            || !self.memory.active_files.is_empty()
            || !self.memory.api_surface.is_empty()
            || !self.memory.recent_failures.is_empty()
            || self.memory.current_task.is_some()
    }
}

impl Default for WorkingMemoryInjector {
    fn default() -> Self {
        Self::with_default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AbstractHistory, FailurePattern, RawCurrent, SubTaskId, TrajectoryState, WorkingMemory,
    };

    #[test]
    fn test_from_working_memory_contains_three_layers() {
        let mut abstract_history = AbstractHistory {
            failure_patterns: Vec::new(),
            root_cause_summary: Some("invalid transition".to_string()),
            attempt_count: 2,
            trajectory_state: TrajectoryState::Cycling {
                repeated_pattern: "same assertion".to_string(),
            },
        };
        abstract_history.failure_patterns.push(FailurePattern {
            error_type: "assertion".to_string(),
            message: "expected completed".to_string(),
            file: None,
            line: None,
            timestamp: chrono::Utc::now(),
        });

        let raw = RawCurrent {
            active_files: vec![std::path::PathBuf::from("src/main.rs")],
            api_surface: Vec::new(),
            current_step_context: None,
        };

        let wm = WorkingMemory::generate(
            SubTaskId::default(),
            Some(abstract_history),
            raw,
            Vec::new(),
        );

        let ctx = WorkingMemoryInjector::from_working_memory(&wm);
        assert!(ctx.abstract_summary.is_some());
        assert!(!ctx.active_files.is_empty());
        assert!(ctx.hard_constraints.is_empty());
    }

    #[test]
    fn test_default_trait_no_recursion() {
        let injector = WorkingMemoryInjector::default();
        assert!(!injector.has_context());
    }
}
