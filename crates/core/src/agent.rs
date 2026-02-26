//! Agent types and roles

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub Uuid);

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn system() -> Self {
        Self(Uuid::nil())
    }
}

/// Agent role in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AgentRole {
    /// Plans tasks and decomposes work
    #[default]
    Planner,
    /// Implements code and executes operations
    Implementer,
    /// Reviews code and validates quality
    Reviewer,
    /// Runs tests and validates results
    Tester,
    /// Records history and manages memory
    Historian,
    /// Administrator with full access
    Admin,
    /// Any role (for access control)
    Any,
    /// System internal operations
    System,
}

/// Agent information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub role: AgentRole,
    pub name: String,
    pub capabilities: Vec<String>,
}

impl Agent {
    pub fn new(id: AgentId, role: AgentRole, name: String) -> Self {
        Self {
            id,
            role,
            name,
            capabilities: Vec::new(),
        }
    }

    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }
}

/// Permission set for an agent role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permissions {
    pub can_read_files: bool,
    pub can_write_files: bool,
    pub can_delete_files: bool,
    pub can_run_commands: bool,
    pub can_read_memory: bool,
    pub can_write_memory: bool,
    pub can_modify_task_state: bool,
    pub can_request_human: bool,
}

impl Permissions {
    pub fn for_role(role: AgentRole) -> Self {
        match role {
            AgentRole::Planner => Permissions {
                can_read_files: true,
                can_write_files: false,
                can_delete_files: false,
                can_run_commands: false,
                can_read_memory: true,
                can_write_memory: false,
                can_modify_task_state: true,
                can_request_human: true,
            },
            AgentRole::Implementer => Permissions {
                can_read_files: true,
                can_write_files: true,
                can_delete_files: false,
                can_run_commands: true,
                can_read_memory: true,
                can_write_memory: false,
                can_modify_task_state: false,
                can_request_human: true,
            },
            AgentRole::Reviewer => Permissions {
                can_read_files: true,
                can_write_files: false,
                can_delete_files: false,
                can_run_commands: true, // for testing
                can_read_memory: true,
                can_write_memory: false,
                can_modify_task_state: true,
                can_request_human: true,
            },
            AgentRole::Tester => Permissions {
                can_read_files: true,
                can_write_files: false,
                can_delete_files: false,
                can_run_commands: true,
                can_read_memory: true,
                can_write_memory: true, // test results
                can_modify_task_state: true,
                can_request_human: true,
            },
            AgentRole::Historian => Permissions {
                can_read_files: true,
                can_write_files: false,
                can_delete_files: false,
                can_run_commands: false,
                can_read_memory: true,
                can_write_memory: true,
                can_modify_task_state: false,
                can_request_human: false,
            },
            AgentRole::Admin => Permissions {
                can_read_files: true,
                can_write_files: true,
                can_delete_files: true,
                can_run_commands: true,
                can_read_memory: true,
                can_write_memory: true,
                can_modify_task_state: true,
                can_request_human: true,
            },
            AgentRole::Any => Permissions {
                can_read_files: false,
                can_write_files: false,
                can_delete_files: false,
                can_run_commands: false,
                can_read_memory: true,
                can_write_memory: false,
                can_modify_task_state: false,
                can_request_human: false,
            },
            AgentRole::System => Permissions {
                can_read_files: true,
                can_write_files: true,
                can_delete_files: true,
                can_run_commands: true,
                can_read_memory: true,
                can_write_memory: true,
                can_modify_task_state: true,
                can_request_human: false,
            },
        }
    }
}
