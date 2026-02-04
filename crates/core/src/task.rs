//! Task management core types

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Unique identifier for a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a task type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskTypeId(pub Uuid);

impl TaskTypeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Task state in the lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskState {
    /// Task is created and waiting to start
    Pending,
    /// Gathering prerequisites
    Preparing,
    /// Task is being executed
    InProgress,
    /// Waiting for verification/tests
    AwaitingVerification,
    /// Blocked by external dependency or human intervention needed
    Blocked,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task was cancelled
    Cancelled,
}

/// Task type definition with state machine template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskType {
    pub id: TaskTypeId,
    pub name: String,
    pub description: String,
    pub default_transitions: Vec<StateTransition>,
    pub default_validation_rules: Vec<ValidationRule>,
    pub role_assignments: RoleAssignments,
}

/// State transition definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    pub from: TaskState,
    pub to: TaskState,
    pub required_conditions: Vec<Condition>,
    pub allowed_roles: Vec<AgentRole>,
    pub auto_transition: bool,
}

/// Condition for state transition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    DependenciesCompleted,
    ArtifactsPresent,
    AllTestsPassed,
    HumanApproved,
    Custom(String),
}

/// Validation rule (composable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    pub id: String,
    pub name: String,
    pub rule_type: ValidationRuleType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationRuleType {
    MustCompile,
    MustPassTests,
    MustPassLint,
    MustHaveDocumentation,
    Custom(String),
}

/// Role assignments for task type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleAssignments {
    pub default_assignee: Option<AgentRole>,
    pub can_complete: Vec<AgentRole>,
    pub can_cancel: Vec<AgentRole>,
}

/// Core task structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub task_type: TaskTypeId,
    pub state: TaskState,
    pub title: String,
    pub description: String,
    pub allowed_transitions: Vec<TaskState>,
    pub required_artifacts: Vec<Artifact>,
    pub validation_rules: Vec<ValidationRule>,
    pub work_records: Vec<WorkRecord>,
    pub dependencies: Vec<TaskId>,
    pub metadata: TaskMetadata,
}

/// Artifact reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub artifact_type: ArtifactType,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArtifactType {
    File,
    Directory,
    TestResult,
    Documentation,
}

/// Work record (append-only log of task operations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkRecord {
    pub id: RecordId,
    pub task_id: TaskId,
    pub agent: AgentId,
    pub operation: Operation,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    StateTransition { from: TaskState, to: TaskState },
    FileEdit { path: String },
    MemoryRead,
    MemoryWrite,
    Custom(String),
}

/// Task metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetadata {
    pub created_at: DateTime<Utc>,
    pub created_by: AgentId,
    pub modified_at: Option<DateTime<Utc>>,
    pub priority: TaskPriority,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Critical,
}

// Re-export from agent module
use crate::agent::{AgentId, AgentRole};
