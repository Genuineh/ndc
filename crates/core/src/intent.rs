//! Intent and Verdict types for the decision system

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::{agent::AgentRole, task::TaskId, memory::MemoryId};

/// Unique identifier for an intent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntentId(pub Uuid);

impl IntentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Intent - AI's proposal for action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub id: IntentId,
    pub agent: AgentId,
    pub agent_role: AgentRole,
    pub proposed_action: Action,
    pub effects: Vec<Effect>,
    pub reasoning: String,
    pub task_id: TaskId,
    pub timestamp: DateTime<Utc>,
}

/// Action that an agent wants to perform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    EditFile { path: String, edits: Vec<FileEdit> },
    CreateFile { path: String, content: String },
    DeleteFile { path: String },
    RunCommand { command: String, args: Vec<String> },
    ReadMemory { query: String },
    WriteMemory { content: String },
    TransitionTask { task_id: TaskId, to: TaskState },
    RequestHumanInput { prompt: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEdit {
    pub start_line: usize,
    pub end_line: usize,
    pub replacement: String,
}

/// Effect - declared impact scope of an intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Effect {
    FileOperation { path: String, op: FileOp },
    TaskTransition { from: TaskState, to: TaskState },
    MemoryOperation { memory_id: MemoryId, op: MemoryOp },
    ToolInvocation { tool: String, args: Vec<String> },
    HumanInteraction { interaction_type: InteractionType },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileOp {
    Read,
    Write,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryOp {
    Read,
    Write,
    Modify,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractionType {
    Question,
    Confirmation,
    Alert,
}

/// Verdict - system's decision on an intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Verdict {
    Allow,
    Deny { reason: String, code: ErrorCode },
    RequireHuman {
        question: String,
        context: HumanContext,
        timeout: Option<u64>, // seconds
    },
    Modify {
        original_action: Action,
        modified_action: Action,
        reason: String,
        warnings: Vec<String>,
    },
    Defer {
        required_info: Vec<InformationRequirement>,
        retry_after: Option<u64>, // seconds
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanContext {
    pub task_id: TaskId,
    pub current_state: TaskState,
    pub proposed_state: TaskState,
    pub relevant_info: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InformationRequirement {
    SecurityApproval(String),
    MissingArtifact(String),
    UnknownDependency(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    ActionNotAllowed,
    InvalidStateTransition,
    PermissionDenied,
    SecurityViolation,
    MissingArtifact,
    DependencyNotMet,
    MemoryConflict,
    AccessDenied,
}

// Re-export types
use crate::{agent::AgentId, task::TaskState};
