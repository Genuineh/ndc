//! Task Lineage Injector
//!
//! Injects task lineage context into Agent prompts
//!
//! Design:
//! - Track parent-child task relationships
//! - Include inherited failures and context
//! - Support context inheritance

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Task lineage entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageEntry {
    /// Current task ID
    pub current_task_id: String,

    /// Parent task ID (if any)
    pub parent_task_id: Option<String>,

    /// Root task ID (origin of this lineage)
    pub root_task_id: String,

    /// Depth in lineage tree
    pub depth: u32,

    /// Inherited invariant IDs
    pub inherited_invariants: Vec<String>,

    /// Inherited failure patterns
    pub inherited_failures: Vec<FailurePattern>,

    /// Inherited context summary
    pub inherited_context: Option<ContextSummary>,

    /// Previous steps taken
    pub previous_steps: Vec<String>,

    /// Branch name if applicable
    pub branch_name: Option<String>,

    /// Commit hash if applicable
    pub commit_hash: Option<String>,
}

impl LineageEntry {
    /// Create new entry
    pub fn new(current_task_id: String, root_task_id: String) -> Self {
        Self {
            current_task_id,
            parent_task_id: None,
            root_task_id,
            depth: 0,
            inherited_invariants: Vec::new(),
            inherited_failures: Vec::new(),
            inherited_context: None,
            previous_steps: Vec::new(),
            branch_name: None,
            commit_hash: None,
        }
    }

    /// Add inherited failure
    pub fn add_failure(&mut self, failure: FailurePattern) {
        self.inherited_failures.push(failure);
    }

    /// Add previous step
    pub fn add_step(&mut self, step: String) {
        self.previous_steps.push(step);
    }
}

/// Failure pattern inherited from lineage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePattern {
    /// Failure description
    pub description: String,

    /// Root cause
    pub root_cause: String,

    /// Solution applied
    pub solution: String,

    /// From which task
    pub from_task_id: String,

    /// Whether this was a human correction
    pub is_human_correction: bool,
}

/// Context summary inherited from parent task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSummary {
    /// Summary text
    pub summary: String,

    /// Key decisions made
    pub key_decisions: Vec<String>,

    /// Files modified
    pub files_modified: Vec<String>,

    /// Important notes
    pub notes: Vec<String>,
}

/// Lineage configuration
#[derive(Debug, Clone)]
pub struct LineageInjectorConfig {
    /// Maximum depth to trace
    pub max_depth: u32,

    /// Include previous steps
    pub include_steps: bool,

    /// Include failure patterns
    pub include_failures: bool,

    /// Include branch/commit info
    pub include_vcs_info: bool,

    /// Include inherited context
    pub include_context: bool,
}

impl Default for LineageInjectorConfig {
    fn default() -> Self {
        Self {
            max_depth: 5,
            include_steps: true,
            include_failures: true,
            include_vcs_info: true,
            include_context: true,
        }
    }
}

/// Task Lineage Injector
#[derive(Debug, Clone)]
pub struct LineageInjector {
    /// Lineage entries by task ID
    lineage: HashMap<String, LineageEntry>,

    /// Root task lineage map
    root_lineage: HashMap<String, Vec<String>>, // root_id -> [task_ids]

    /// Configuration
    config: LineageInjectorConfig,
}

impl LineageInjector {
    /// Create new injector
    pub fn new(config: LineageInjectorConfig) -> Self {
        Self {
            lineage: HashMap::new(),
            root_lineage: HashMap::new(),
            config,
        }
    }

    /// Add a lineage entry
    pub fn add_lineage(&mut self, entry: LineageEntry) {
        // Track in root lineage
        self.root_lineage
            .entry(entry.root_task_id.clone())
            .or_default()
            .push(entry.current_task_id.clone());

        self.lineage.insert(entry.current_task_id.clone(), entry);
    }

    /// Create child task lineage from parent
    pub fn create_child_lineage(
        &mut self,
        child_task_id: String,
        parent_task_id: &str,
        root_task_id: String,
    ) -> LineageEntry {
        let parent = self.lineage.get(parent_task_id).cloned();

        let depth = parent.as_ref().map(|p| p.depth + 1).unwrap_or(0);

        let mut entry = LineageEntry::new(child_task_id.clone(), root_task_id.clone());

        if let Some(ref parent) = parent {
            entry.parent_task_id = Some(parent_task_id.to_string());
            entry.depth = depth.min(self.config.max_depth);

            // Inherit invariants
            entry.inherited_invariants = parent.inherited_invariants.clone();

            // Inherit failures (if configured)
            if self.config.include_failures {
                entry.inherited_failures = parent.inherited_failures.clone();
            }

            // Inherit context (if configured)
            if self.config.include_context {
                entry.inherited_context = parent.inherited_context.clone();
            }

            // Inherit previous steps (if configured)
            if self.config.include_steps {
                entry.previous_steps = parent.previous_steps.clone();
            }

            // Inherit VCS info
            if self.config.include_vcs_info {
                entry.branch_name = parent.branch_name.clone();
                entry.commit_hash = parent.commit_hash.clone();
            }
        }

        self.add_lineage(entry.clone());
        entry
    }

    /// Add failure to lineage
    pub fn add_failure(&mut self, task_id: &str, failure: FailurePattern) {
        if let Some(entry) = self.lineage.get_mut(task_id) {
            entry.add_failure(failure);
        }
    }

    /// Add previous step
    pub fn add_step(&mut self, task_id: &str, step: String) {
        if let Some(entry) = self.lineage.get_mut(task_id) {
            entry.add_step(step);
        }
    }

    /// Get lineage for a task
    pub fn get_lineage(&self, task_id: &str) -> Option<&LineageEntry> {
        self.lineage.get(task_id)
    }

    /// Get full lineage chain for a root task
    pub fn get_lineage_chain(&self, root_task_id: &str) -> Vec<&LineageEntry> {
        if let Some(task_ids) = self.root_lineage.get(root_task_id) {
            task_ids
                .iter()
                .filter_map(|id| self.lineage.get(id))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Inject as prompt text
    pub fn inject(&self, task_id: &str) -> String {
        let entry = match self.lineage.get(task_id) {
            Some(e) => e,
            None => {
                return "=== TASK LINEAGE ===\n(no lineage information)\n=== END LINEAGE ==="
                    .to_string();
            }
        };

        let mut lines = Vec::new();

        lines.push("=== TASK LINEAGE ===".to_string());

        // Basic info
        lines.push(format!("Task: {}", entry.current_task_id));
        if let Some(ref parent) = entry.parent_task_id {
            lines.push(format!("Parent: {}", parent));
        }
        lines.push(format!("Root: {}", entry.root_task_id));
        lines.push(format!("Depth: {}", entry.depth));

        // VCS info
        if self.config.include_vcs_info {
            if let Some(ref branch) = entry.branch_name {
                lines.push(format!("Branch: {}", branch));
            }
            if let Some(ref commit) = entry.commit_hash {
                lines.push(format!("Commit: {}", commit));
            }
        }

        // Previous steps
        if self.config.include_steps && !entry.previous_steps.is_empty() {
            lines.push("\nPrevious Steps:".to_string());
            for (i, step) in entry.previous_steps.iter().enumerate() {
                lines.push(format!("  {}. {}", i + 1, step));
            }
        }

        // Inherited failures
        if self.config.include_failures && !entry.inherited_failures.is_empty() {
            lines.push("\nInherited Failures (learned from lineage):".to_string());
            for failure in &entry.inherited_failures {
                lines.push(format!(
                    "  • {}: {}",
                    failure.description, failure.root_cause
                ));
                lines.push(format!("    Solution: {}", failure.solution));
            }
        }

        // Inherited context
        if self.config.include_context
            && let Some(ref ctx) = entry.inherited_context
        {
            lines.push("\nInherited Context:".to_string());
            lines.push(format!("  {}", ctx.summary));
            if !ctx.key_decisions.is_empty() {
                lines.push("\n  Key Decisions:".to_string());
                for decision in &ctx.key_decisions {
                    lines.push(format!("    • {}", decision));
                }
            }
        }

        // Inherited invariants count
        if !entry.inherited_invariants.is_empty() {
            lines.push(format!(
                "\nInherited Invariants: {}",
                entry.inherited_invariants.len()
            ));
        }

        lines.push("=== END LINEAGE ===".to_string());

        lines.join("\n")
    }

    /// Inject as structured data
    pub fn inject_json(&self, task_id: &str) -> Value {
        if let Some(entry) = self.lineage.get(task_id) {
            json!({
                "type": "lineage",
                "task_id": entry.current_task_id,
                "parent_id": entry.parent_task_id,
                "root_id": entry.root_task_id,
                "depth": entry.depth,
                "inherited_invariants": entry.inherited_invariants,
                "inherited_failures": entry.inherited_failures.iter().map(|f| json!({
                    "description": f.description,
                    "root_cause": f.root_cause,
                    "solution": f.solution,
                })).collect::<Vec<_>>(),
                "previous_steps": entry.previous_steps,
            })
        } else {
            json!({
                "type": "lineage",
                "task_id": task_id,
                "has_lineage": false,
            })
        }
    }
}

impl Default for LineageInjector {
    fn default() -> Self {
        Self::new(LineageInjectorConfig::default())
    }
}
