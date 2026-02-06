//! NDC Runtime - Execution engine
//!
//! Responsibilities:
//! - Tool management (fs, shell)
//! - Quality gate execution
//! - Task execution
//! - Workflow management
//! - Storage

pub mod tools;
pub mod verify;
pub mod storage;
#[cfg(feature = "sqlite")]
pub mod storage_sqlite;
pub mod workflow;
pub mod executor;

pub use tools::{Tool, ToolResult, ToolError, ToolManager, ToolContext};
pub use verify::QualityGateRunner;
pub use storage::{Storage, MemoryStorage, SharedStorage, create_memory_storage};
#[cfg(feature = "sqlite")]
pub use storage_sqlite::{SqliteStorage, SqliteStorageError, create_sqlite_storage};
pub use workflow::{WorkflowEngine, WorkflowListener, WorkflowError};
pub use executor::{Executor, ExecutionContext, ExecutionResult, ExecutionError};
