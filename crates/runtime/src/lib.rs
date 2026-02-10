//! NDC Runtime - Execution engine
//!
//! Responsibilities:
//! - Tool management (fs, shell)
//! - Quality gate execution
//! - Task execution
//! - Workflow management
//! - Storage
//! - Discovery phase (impact analysis, volatility heatmap, hard constraints)
//! - Saga Pattern (rollback support)

pub mod tools;
pub mod verify;
pub mod storage;
#[cfg(feature = "sqlite")]
pub mod storage_sqlite;
pub mod workflow;
pub mod executor;
pub mod discovery;
pub mod execution;
pub mod engine;
pub mod mcp;
pub mod skill;

pub use tools::{Tool, ToolResult, ToolError, ToolManager, ToolContext};
pub use verify::QualityGateRunner;
pub use storage::{Storage, MemoryStorage, SharedStorage, create_memory_storage};
#[cfg(feature = "sqlite")]
pub use storage_sqlite::{SqliteStorage, SqliteStorageError, create_sqlite_storage};
pub use workflow::{WorkflowEngine, WorkflowListener, WorkflowError};
pub use executor::{Executor, ExecutionContext, ExecutionResult, ExecutionError};
pub use discovery::{
    DiscoveryService,
    DiscoveryConfig,
    DiscoveryResult,
    DiscoveryError,
    VolatilityHeatmap,
    HeatmapConfig,
    ModuleId,
    HardConstraints,
    ImpactReport,
    ImpactScope,
    Complexity,
};
pub use execution::{
    SagaPlan,
    SagaStep,
    SagaId,
    StepId,
    UndoAction,
    CompensationAction,
    StepStatus,
    RollbackError,
    SagaSummary,
};
pub use engine::{
    EventEngine,
    EventEmitter,
    Event,
    EventType,
    EventId,
    EventData,
    EventListener,
    Workflow,
    WorkflowState,
    TransitionError,
    EventEngineSummary,
};
pub use mcp::{
    McpManager,
    McpServerConfig,
    McpServerType,
    McpTool,
    McpPrompt,
    McpResource,
    McpResult,
};
pub use skill::{
    Skill,
    SkillRegistry,
    SkillParameter,
    SkillExample,
};
