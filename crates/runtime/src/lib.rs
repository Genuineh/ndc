//! NDC Runtime - Execution engine
//!
//! Responsibilities:
//! - Tool management (fs, shell)
//! - Quality gate execution
//! - Task execution
//! - Workflow management
//! - Discovery phase (impact analysis, volatility heatmap, hard constraints)
//! - Saga Pattern (rollback support)
//!
//! Storage is provided by the `ndc-storage` crate and re-exported here for convenience.

pub mod discovery;
pub mod documentation;
pub mod engine;
pub mod execution;
pub mod executor;
pub mod mcp;
pub mod skill;
pub mod tools;
pub mod verify;
pub mod workflow;

// Re-export storage from ndc-storage crate
pub use ndc_storage::{create_memory_storage, MemoryStorage, SharedStorage, Storage};
#[cfg(feature = "sqlite")]
pub use ndc_storage::{create_sqlite_storage, SqliteStorage, SqliteStorageError};

pub use discovery::{
    Complexity, DiscoveryConfig, DiscoveryError, DiscoveryResult, DiscoveryService,
    HardConstraints, HeatmapConfig, ImpactReport, ImpactScope, ModuleId, VolatilityHeatmap,
};
pub use documentation::{
    DocUpdateRequest, DocUpdateResult, DocUpdateType, DocUpdater, DocUpdaterConfig, Fact,
    FactCategory, Narrative,
};
pub use engine::{
    Event, EventData, EventEmitter, EventEngine, EventEngineSummary, EventId, EventListener,
    EventType, TransitionError, Workflow, WorkflowState,
};
pub use execution::{
    CompensationAction, RollbackError, SagaId, SagaPlan, SagaStep, SagaSummary, StepId, StepStatus,
    UndoAction,
};
pub use executor::{ExecutionContext, ExecutionError, ExecutionResult, Executor};
pub use mcp::{
    McpManager, McpPrompt, McpResource, McpResult, McpServerConfig, McpServerType, McpTool,
};
pub use skill::{Skill, SkillExample, SkillParameter, SkillRegistry};
pub use tools::{
    create_default_tool_manager, create_default_tool_manager_with_storage,
    create_default_tool_registry, create_default_tool_registry_with_storage, Tool, ToolContext,
    ToolError, ToolManager, ToolResult,
};
pub use verify::QualityGateRunner;
pub use workflow::{WorkflowEngine, WorkflowError, WorkflowListener};
