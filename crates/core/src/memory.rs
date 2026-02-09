//! Memory types and stability levels

mod working_memory;

pub use working_memory::*;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::HashSet;

use crate::agent::{AgentId, AgentRole};
use crate::TaskId;

/// Unique identifier for a memory entry (uses UUID for compatibility)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryId(pub Uuid);

impl MemoryId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Memory stability level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MemoryStability {
    /// Temporary reasoning (may be overturned)
    Ephemeral = 0,
    /// Derived conclusion (not yet verified)
    Derived = 1,
    /// Verified (by tests or human)
    Verified = 2,
    /// Fact/constraint (system-level truth)
    Canonical = 3,
}

/// Memory content types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryContent {
    Code(CodeKnowledge),
    ProjectStructure(ProjectStructure),
    ApiDocumentation(ApiDoc),
    Decision(DecisionRecord),
    ErrorSolution(ErrorSolution),
    TestResult(TestResult),
    General { text: String, metadata: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeKnowledge {
    pub file_path: String,
    pub language: String,
    pub summary: String,
    pub functions: Vec<FunctionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub signature: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStructure {
    pub root_path: String,
    pub directories: Vec<String>,
    pub important_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiDoc {
    pub endpoint: String,
    pub method: String,
    pub parameters: Vec<ParameterInfo>,
    pub return_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterInfo {
    pub name: String,
    pub param_type: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub decision: String,
    pub rationale: String,
    pub alternatives: Vec<String>,
    pub made_by: AgentId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSolution {
    pub error: String,
    pub solution: String,
    pub prevention: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub test_name: String,
    pub passed: bool,
    pub output: String,
    pub timestamp: DateTime<Utc>,
}

/// Relation between memory entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub target: MemoryId,
    pub relation_type: RelationType,
    pub strength: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelationType {
    Dependency,
    Reference,
    Implementation,
    Related,
    Contradicts,
}

/// Memory entry metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetadata {
    pub stability: MemoryStability,
    pub created_at: DateTime<Utc>,
    pub created_by: AgentId,
    pub source_task: TaskId,
    pub version: u64,
    pub modified_at: Option<DateTime<Utc>>,
    pub tags: Vec<String>,
}

// Use TaskId from task module (re-exported from lib.rs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControl {
    pub owner: AgentId,
    pub read_roles: HashSet<AgentRole>,
    pub write_roles: HashSet<AgentRole>,
    pub created_at: DateTime<Utc>,
    pub modified_at: Option<DateTime<Utc>>,
}

impl AccessControl {
    pub fn new(owner: AgentId, stability: MemoryStability) -> Self {
        let read_roles: HashSet<AgentRole> = [
            AgentRole::Planner,
            AgentRole::Implementer,
            AgentRole::Reviewer,
            AgentRole::Tester,
            AgentRole::Historian,
            AgentRole::Admin,
        ].iter().cloned().collect();

        let write_roles: HashSet<AgentRole> = match stability {
            MemoryStability::Ephemeral => [
                AgentRole::Implementer,
                AgentRole::Historian,
                AgentRole::Admin,
            ].iter().cloned().collect(),
            MemoryStability::Derived => [
                AgentRole::Historian,
                AgentRole::Admin,
            ].iter().cloned().collect(),
            MemoryStability::Verified => [
                AgentRole::Historian,
                AgentRole::Admin,
            ].iter().cloned().collect(),
            MemoryStability::Canonical => [
                AgentRole::Admin,
            ].iter().cloned().collect(),
        };

        Self {
            owner,
            read_roles,
            write_roles,
            created_at: Utc::now(),
            modified_at: None,
        }
    }

    pub fn allow_read(&self, role: &AgentRole) -> bool {
        self.read_roles.contains(role) || self.read_roles.contains(&AgentRole::Any)
    }

    pub fn allow_write(&self, role: &AgentRole) -> bool {
        self.write_roles.contains(role)
    }
}

/// Memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub content: MemoryContent,
    pub embedding: Vec<f32>,
    pub relations: Vec<Relation>,
    pub metadata: MemoryMetadata,
    pub access_control: AccessControl,
}

impl MemoryEntry {
    pub fn stability(&self) -> &MemoryStability {
        &self.metadata.stability
    }

    pub fn id(&self) -> MemoryId {
        self.id
    }
}

/// Memory query for filtering memories
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryQuery {
    /// Text query for filtering
    pub query: Option<String>,

    /// Filter by stability level
    pub stability: Option<MemoryStability>,

    /// Filter by memory type
    pub memory_type: Option<String>,

    /// Filter by tags
    pub tags: Vec<String>,

    /// Filter by source task
    pub source_task: Option<TaskId>,

    /// Filter by minimum stability (inclusive)
    pub min_stability: Option<MemoryStability>,

    /// Filter by maximum stability (inclusive)
    pub max_stability: Option<MemoryStability>,
}

/// A memory with a similarity score (for vector search results)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredMemory {
    pub memory: MemoryEntry,
    pub score: f32,
}

/// Type alias for Memory (used by persistence layer)
pub type Memory = MemoryEntry;

