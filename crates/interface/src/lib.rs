//! NDC Interface - 交互层
//!
//! 职责：
//! - CLI 命令行工具
//! - REPL 交互模式
//! - gRPC 守护进程
//!
//! 架构：
//! - cli/: 命令行接口
//! - repl/: 交互式 REPL
//! - daemon/: gRPC 服务

mod agent_backend_impl;
pub mod agent_mode;
pub mod cli;
pub mod daemon;
pub mod interactive;
pub(crate) mod permission_engine;
pub(crate) mod project_index;
pub(crate) mod provider_config;
pub mod redaction;
pub mod repl;
pub(crate) mod session_archive;

#[cfg(feature = "grpc")]
pub mod generated;

#[cfg(feature = "grpc")]
pub mod grpc_client;

#[cfg(feature = "grpc")]
pub mod grpc;

#[cfg(test)]
mod cli_tests;

#[cfg(test)]
mod daemon_tests;

#[cfg(test)]
mod e2e_tests;

pub use agent_mode::{
    AgentModeConfig, AgentModeManager, AgentModeState, AgentModeStatus, PermissionRule,
    handle_agent_command, show_agent_status,
};
pub use cli::{CliConfig, run};
pub use daemon::run_daemon;
pub use repl::{ReplConfig, ReplState, run_repl};

// Interactive components
pub use interactive::{
    AgentModeSwitcher, AgentSwitchResult, MultiProgress, PermissionConfirm, PermissionResult,
    ProgressIndicator, RiskLevel, StreamingDisplay, display_agent_status, display_tool_call,
    prompt_recovery,
};

#[cfg(feature = "grpc")]
pub use grpc_client::{ClientConfig, ClientError, NdcClient, create_client};
