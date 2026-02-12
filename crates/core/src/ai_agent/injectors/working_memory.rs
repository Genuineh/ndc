//! Working Memory Injector
//!
//! Injects current working memory context into Agent prompts
//!
//! Design:
//! - Extract relevant context from WorkingMemory
//! - Format as readable text for the LLM
//! - Inject only relevant information based on task

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Working Memory context for agent injection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingMemoryContext {
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

impl Default for WorkingMemoryContext {
    fn default() -> Self {
        Self {
            active_files: Vec::new(),
            api_surface: Vec::new(),
            recent_failures: Vec::new(),
            current_task: None,
            custom: HashMap::new(),
        }
    }
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
    pub fn default() -> Self {
        Self::new(WorkingMemoryInjectorConfig::default())
    }

    /// Update working memory
    pub fn update(&mut self, memory: WorkingMemoryContext) {
        self.memory = memory;
    }

    /// Set active files
    pub fn set_active_files(&mut self, files: Vec<String>) {
        self.memory.active_files = files.into_iter().take(self.config.max_active_files).collect();
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
        if self.config.include_task_context {
            if let Some(ref task) = self.memory.current_task {
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
        !self.memory.active_files.is_empty()
            || !self.memory.api_surface.is_empty()
            || !self.memory.recent_failures.is_empty()
            || self.memory.current_task.is_some()
    }
}

impl Default for WorkingMemoryInjector {
    fn default() -> Self {
        Self::default()
    }
}
