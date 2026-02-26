//! Decomposition Lint - Non-LLM Deterministic Validation
//!
//! Validates LLM task decomposition results against engineering rules.
//! Ensures decomposition meets quality standards without relying on LLM.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Decomposition Lint Rules
pub struct DecompositionLint {
    /// Lint rules to apply
    rules: Vec<Box<dyn LintRule>>,
}

impl Clone for DecompositionLint {
    fn clone(&self) -> Self {
        // Create new instance with default rules
        Self::new()
    }
}

impl std::fmt::Debug for DecompositionLint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DecompositionLint with {} rules", self.rules.len())
    }
}

/// Individual lint rule
pub trait LintRule: Send + Sync {
    /// Rule name
    fn name(&self) -> &str;

    /// Rule description
    fn description(&self) -> &str;

    /// Check if decomposition passes this rule
    fn check(&self, decomposition: &TaskDecomposition) -> Vec<LintViolation>;
}

/// Lint result
#[derive(Debug, Clone)]
pub struct LintResult {
    /// All violations found
    pub violations: Vec<LintViolation>,

    /// Passed all checks?
    pub passed: bool,

    /// Overall severity
    pub severity: LintSeverity,
}

/// Lint violation
#[derive(Debug, Clone)]
pub struct LintViolation {
    /// Rule that was violated
    pub rule: String,

    /// Violation message
    pub message: String,

    /// Severity
    pub severity: LintSeverity,

    /// Affected subtasks
    pub affected_subtasks: Vec<String>,

    /// Fix suggestion
    pub suggestion: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LintSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Task decomposition from LLM
#[derive(Debug, Clone)]
pub struct TaskDecomposition {
    /// Original task ID
    pub task_id: String,

    /// Root TODO reference
    pub root_todo_id: Option<String>,

    /// Subtasks
    pub subtasks: Vec<SubTask>,

    /// Dependencies between subtasks
    pub dependencies: Vec<TaskDependency>,
}

/// Subtask in decomposition
#[derive(Debug, Clone)]
pub struct SubTask {
    /// Subtask ID
    pub id: String,

    /// Title
    pub title: String,

    /// Description
    pub description: String,

    /// Expected action type
    pub action_type: ActionType,

    /// Estimated complexity
    pub complexity: Complexity,

    /// Dependencies
    pub depends_on: Vec<String>,

    /// Expected files
    pub expected_files: Vec<PathBuf>,

    /// Verification criteria
    pub verification: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    CreateFile,
    ModifyFile,
    DeleteFile,
    Refactor,
    Test,
    Document,
    Review,
    Config,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Complexity {
    Trivial,
    Simple,
    Moderate,
    Complex,
    VeryComplex,
}

/// Dependency between tasks
#[derive(Debug, Clone)]
pub struct TaskDependency {
    /// Dependent task
    pub from: String,

    /// Task it depends on
    pub to: String,

    /// Dependency type
    pub dependency_type: DependencyType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyType {
    MustCompleteBefore,
    ShouldCompleteBefore,
    CanRunAfter,
}

impl Default for DecompositionLint {
    fn default() -> Self {
        Self::new()
    }
}

impl DecompositionLint {
    /// Create with default rules
    pub fn new() -> Self {
        let rules: Vec<Box<dyn LintRule>> = vec![
            Box::new(CyclicDependencyRule),
            Box::new(MissingVerificationRule),
            Box::new(TooComplexRule),
            Box::new(OrphanedTaskRule),
            Box::new(MissingFilesRule),
            Box::new(TooManySubtasksRule),
        ];

        Self { rules }
    }

    /// Run all lint checks
    pub fn check(&self, decomposition: &TaskDecomposition) -> LintResult {
        let mut all_violations = Vec::new();

        for rule in &self.rules {
            let violations = rule.check(decomposition);
            all_violations.extend(violations);
        }

        let passed = all_violations
            .iter()
            .all(|v| v.severity != LintSeverity::Error && v.severity != LintSeverity::Critical);

        let severity = if all_violations
            .iter()
            .any(|v| v.severity == LintSeverity::Critical)
        {
            LintSeverity::Critical
        } else if all_violations
            .iter()
            .any(|v| v.severity == LintSeverity::Error)
        {
            LintSeverity::Error
        } else if all_violations
            .iter()
            .any(|v| v.severity == LintSeverity::Warning)
        {
            LintSeverity::Warning
        } else {
            LintSeverity::Info
        };

        LintResult {
            violations: all_violations,
            passed,
            severity,
        }
    }
}

/// Helper function for DFS cycle detection
fn dfs_cycle_check(
    node: &String,
    adj: &std::collections::HashMap<String, Vec<String>>,
    visited: &mut std::collections::HashSet<String>,
    path: &mut std::collections::HashSet<String>,
    violations: &mut Vec<LintViolation>,
) {
    visited.insert(node.clone());
    path.insert(node.clone());

    if let Some(neighbors) = adj.get(node) {
        for neighbor in neighbors {
            if path.contains(neighbor) {
                // Cycle detected!
                violations.push(LintViolation {
                    rule: "cyclic-dependency".to_string(),
                    message: format!("Cyclic dependency detected: {} -> {}", node, neighbor),
                    severity: LintSeverity::Critical,
                    affected_subtasks: vec![node.clone(), neighbor.clone()],
                    suggestion: "Remove circular dependency by reordering tasks".to_string(),
                });
            } else if !visited.contains(neighbor) {
                dfs_cycle_check(neighbor, adj, visited, path, violations);
            }
        }
    }

    path.remove(node);
}

/// Rule: Detect cyclic dependencies
#[derive(Debug, Clone)]
pub struct CyclicDependencyRule;

impl LintRule for CyclicDependencyRule {
    fn name(&self) -> &str {
        "cyclic-dependency"
    }

    fn description(&self) -> &str {
        "Detects circular dependencies between subtasks"
    }

    fn check(&self, decomposition: &TaskDecomposition) -> Vec<LintViolation> {
        let mut violations = Vec::new();

        // Build adjacency list
        let adj: std::collections::HashMap<String, Vec<String>> = decomposition
            .dependencies
            .iter()
            .fold(std::collections::HashMap::new(), |mut acc, d| {
                acc.entry(d.from.clone()).or_default().push(d.to.clone());
                acc
            });

        // Get all unique nodes
        let nodes: std::collections::HashSet<String> = decomposition
            .dependencies
            .iter()
            .flat_map(|d| vec![d.from.clone(), d.to.clone()])
            .collect();

        // DFS with path tracking for cycle detection
        let mut visited = std::collections::HashSet::new();
        let mut path = std::collections::HashSet::new();

        for start in &nodes {
            if !visited.contains(start) {
                dfs_cycle_check(start, &adj, &mut visited, &mut path, &mut violations);
            }
        }

        violations
    }
}

/// Rule: Verify each task has verification criteria
#[derive(Debug, Clone)]
pub struct MissingVerificationRule;

impl LintRule for MissingVerificationRule {
    fn name(&self) -> &str {
        "missing-verification"
    }

    fn description(&self) -> &str {
        "Ensures each subtask has verification criteria"
    }

    fn check(&self, decomposition: &TaskDecomposition) -> Vec<LintViolation> {
        decomposition
            .subtasks
            .iter()
            .filter(|t| t.verification.is_empty())
            .map(|task| LintViolation {
                rule: self.name().to_string(),
                message: format!("Subtask '{}' has no verification criteria", task.title),
                severity: LintSeverity::Warning,
                affected_subtasks: vec![task.id.clone()],
                suggestion:
                    "Add verification criteria (e.g., 'cargo test passes', 'code compiles')"
                        .to_string(),
            })
            .collect()
    }
}

/// Rule: Detect overly complex subtasks
#[derive(Debug, Clone)]
pub struct TooComplexRule;

impl LintRule for TooComplexRule {
    fn name(&self) -> &str {
        "too-complex"
    }

    fn description(&self) -> &str {
        "Detects subtasks that are too complex"
    }

    fn check(&self, decomposition: &TaskDecomposition) -> Vec<LintViolation> {
        decomposition
            .subtasks
            .iter()
            .filter(|t| matches!(t.complexity, Complexity::VeryComplex))
            .map(|task| LintViolation {
                rule: self.name().to_string(),
                message: format!("Subtask '{}' is marked as VeryComplex", task.title),
                severity: LintSeverity::Warning,
                affected_subtasks: vec![task.id.clone()],
                suggestion: "Consider breaking into smaller subtasks".to_string(),
            })
            .collect()
    }
}

/// Rule: Detect orphaned tasks (no dependencies but could have)
#[derive(Debug, Clone)]
pub struct OrphanedTaskRule;

impl LintRule for OrphanedTaskRule {
    fn name(&self) -> &str {
        "orphaned-task"
    }

    fn description(&self) -> &str {
        "Detects tasks that may have undeclared dependencies"
    }

    fn check(&self, decomposition: &TaskDecomposition) -> Vec<LintViolation> {
        let mut violations = Vec::new();

        // Group tasks by file
        let mut file_groups: std::collections::HashMap<&PathBuf, Vec<&str>> =
            std::collections::HashMap::new();
        for task in &decomposition.subtasks {
            for file in &task.expected_files {
                file_groups.entry(file).or_default().push(task.id.as_str());
            }
        }

        // For each file, check if tasks modifying it have proper dependencies
        for (file, task_ids) in &file_groups {
            // Only flag if multiple tasks modify the same file
            if task_ids.len() > 1 {
                for (i, task1_id) in task_ids.iter().enumerate() {
                    for task2_id in task_ids.iter().skip(i + 1) {
                        let has_dep = decomposition.dependencies.iter().any(|d| {
                            (d.from == *task1_id && d.to == *task2_id)
                                || (d.from == *task2_id && d.to == *task1_id)
                        });

                        if !has_dep {
                            violations.push(LintViolation {
                                rule: self.name().to_string(),
                                message: format!(
                                    "Tasks '{}' and '{}' modify same file '{}' without dependency",
                                    task1_id,
                                    task2_id,
                                    file.display()
                                ),
                                severity: LintSeverity::Info,
                                affected_subtasks: vec![task1_id.to_string(), task2_id.to_string()],
                                suggestion: "Consider adding dependency between these tasks"
                                    .to_string(),
                            });
                        }
                    }
                }
            }
        }

        violations
    }
}

/// Rule: Check for expected files
#[derive(Debug, Clone)]
pub struct MissingFilesRule;

impl LintRule for MissingFilesRule {
    fn name(&self) -> &str {
        "missing-files"
    }

    fn description(&self) -> &str {
        "Validates that subtasks have expected file modifications"
    }

    fn check(&self, decomposition: &TaskDecomposition) -> Vec<LintViolation> {
        decomposition
            .subtasks
            .iter()
            .filter(|t| !matches!(t.action_type, ActionType::Review | ActionType::Config))
            .filter(|t| t.expected_files.is_empty())
            .map(|task| LintViolation {
                rule: self.name().to_string(),
                message: format!("Subtask '{}' expects no file modifications", task.title),
                severity: LintSeverity::Warning,
                affected_subtasks: vec![task.id.clone()],
                suggestion: "Specify expected files or clarify why none are needed".to_string(),
            })
            .collect()
    }
}

/// Rule: Check subtask count
#[derive(Debug, Clone)]
pub struct TooManySubtasksRule;

impl LintRule for TooManySubtasksRule {
    fn name(&self) -> &str {
        "too-many-subtasks"
    }

    fn description(&self) -> &str {
        "Detects decompositions with excessive subtasks"
    }

    fn check(&self, decomposition: &TaskDecomposition) -> Vec<LintViolation> {
        let max_subtasks = 20;

        if decomposition.subtasks.len() > max_subtasks {
            vec![LintViolation {
                rule: self.name().to_string(),
                message: format!(
                    "Decomposition has {} subtasks (max: {})",
                    decomposition.subtasks.len(),
                    max_subtasks
                ),
                severity: LintSeverity::Warning,
                affected_subtasks: decomposition
                    .subtasks
                    .iter()
                    .map(|t| t.id.clone())
                    .collect(),
                suggestion: "Consider if this could be split into multiple tasks".to_string(),
            }]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cyclic_dependency_detected() {
        let decomposition = TaskDecomposition {
            task_id: "task-1".to_string(),
            root_todo_id: None,
            subtasks: vec![
                SubTask {
                    id: "a".to_string(),
                    title: "Task A".to_string(),
                    description: "Does A".to_string(),
                    action_type: ActionType::CreateFile,
                    complexity: Complexity::Simple,
                    depends_on: vec!["b".to_string()],
                    expected_files: vec![],
                    verification: vec!["compiles".to_string()],
                },
                SubTask {
                    id: "b".to_string(),
                    title: "Task B".to_string(),
                    description: "Does B".to_string(),
                    action_type: ActionType::CreateFile,
                    complexity: Complexity::Simple,
                    depends_on: vec!["a".to_string()],
                    expected_files: vec![],
                    verification: vec!["compiles".to_string()],
                },
            ],
            dependencies: vec![
                TaskDependency {
                    from: "a".to_string(),
                    to: "b".to_string(),
                    dependency_type: DependencyType::MustCompleteBefore,
                },
                TaskDependency {
                    from: "b".to_string(),
                    to: "a".to_string(),
                    dependency_type: DependencyType::MustCompleteBefore,
                },
            ],
        };

        let lint = DecompositionLint::new();
        let result = lint.check(&decomposition);

        assert!(!result.passed);
        assert!(result.severity == LintSeverity::Critical);
    }

    #[test]
    fn test_missing_verification() {
        let decomposition = TaskDecomposition {
            task_id: "task-1".to_string(),
            root_todo_id: None,
            subtasks: vec![
                SubTask {
                    id: "a".to_string(),
                    title: "Task A".to_string(),
                    description: "Does A".to_string(),
                    action_type: ActionType::CreateFile,
                    complexity: Complexity::Simple,
                    depends_on: vec![],
                    expected_files: vec![],
                    verification: vec!["compiles".to_string()],
                },
                SubTask {
                    id: "b".to_string(),
                    title: "Task B".to_string(),
                    description: "Does B".to_string(),
                    action_type: ActionType::ModifyFile,
                    complexity: Complexity::Simple,
                    depends_on: vec![],
                    expected_files: vec![],
                    verification: vec![],
                },
            ],
            dependencies: Vec::new(),
        };

        let lint = DecompositionLint::new();
        let result = lint.check(&decomposition);

        let missing_verification = result
            .violations
            .iter()
            .any(|v| v.rule == "missing-verification");

        assert!(missing_verification);
    }

    #[test]
    fn test_too_complex() {
        let decomposition = TaskDecomposition {
            task_id: "task-1".to_string(),
            root_todo_id: None,
            subtasks: vec![SubTask {
                id: "a".to_string(),
                title: "Complex Task".to_string(),
                description: "Very complex".to_string(),
                action_type: ActionType::Refactor,
                complexity: Complexity::VeryComplex,
                depends_on: vec![],
                expected_files: vec![],
                verification: vec!["all tests pass".to_string()],
            }],
            dependencies: Vec::new(),
        };

        let lint = DecompositionLint::new();
        let result = lint.check(&decomposition);

        let too_complex = result.violations.iter().any(|v| v.rule == "too-complex");

        assert!(too_complex);
    }

    #[test]
    fn test_valid_decomposition() {
        let decomposition = TaskDecomposition {
            task_id: "task-1".to_string(),
            root_todo_id: None,
            subtasks: vec![
                SubTask {
                    id: "a".to_string(),
                    title: "Setup".to_string(),
                    description: "Create setup".to_string(),
                    action_type: ActionType::CreateFile,
                    complexity: Complexity::Simple,
                    depends_on: vec![],
                    expected_files: vec![PathBuf::from("setup.rs")],
                    verification: vec!["compiles".to_string()],
                },
                SubTask {
                    id: "b".to_string(),
                    title: "Main".to_string(),
                    description: "Implement main".to_string(),
                    action_type: ActionType::ModifyFile,
                    complexity: Complexity::Moderate,
                    depends_on: vec!["a".to_string()],
                    expected_files: vec![PathBuf::from("main.rs")],
                    verification: vec!["tests pass".to_string()],
                },
            ],
            dependencies: vec![TaskDependency {
                from: "b".to_string(),
                to: "a".to_string(),
                dependency_type: DependencyType::MustCompleteBefore,
            }],
        };

        let lint = DecompositionLint::new();
        let result = lint.check(&decomposition);

        assert!(result.passed);
    }

    #[test]
    fn test_lint_summary() {
        let decomposition = TaskDecomposition {
            task_id: "task-1".to_string(),
            root_todo_id: None,
            subtasks: (0..25)
                .map(|i| SubTask {
                    id: format!("task-{}", i),
                    title: format!("Task {}", i),
                    description: "Description".to_string(),
                    action_type: ActionType::CreateFile,
                    complexity: Complexity::Simple,
                    depends_on: vec![],
                    expected_files: vec![],
                    verification: vec!["test".to_string()],
                })
                .collect(),
            dependencies: Vec::new(),
        };

        let lint = DecompositionLint::new();
        let result = lint.check(&decomposition);

        let too_many = result
            .violations
            .iter()
            .any(|v| v.rule == "too-many-subtasks");

        assert!(too_many);
    }
}
