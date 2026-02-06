//! NDC Runtime - 执行与验证引擎
//!
//! 职责：
//! - 任务调度与执行
//! - 受控工具集
//! - 质量门禁
//! - 状态机流转
//!
//! 架构：
//! - Executor: 异步任务调度器
//! - tools/: 受控工具集（fs, git, shell）
//! - verify/: 质量门禁（tests, lint, build）

pub mod executor;
pub mod workflow;
pub mod tools;
pub mod verify;

pub use executor::{Executor, ExecutionContext, ExecutionResult};
pub use workflow::{WorkflowEngine, WorkflowStep, WorkflowEvent};
pub use tools::{Tool, ToolResult, ToolError};
pub use verify::{QualityGate, QualityCheck, QualityResult};
