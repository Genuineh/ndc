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

pub mod agent_mode;
pub mod cli;
pub mod daemon;
pub mod interactive;
pub mod redaction;
pub mod repl;

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
    handle_agent_command, show_agent_status, AgentModeConfig, AgentModeManager, AgentModeState,
    AgentModeStatus, PermissionRule,
};
pub use cli::{run, CliConfig};
pub use daemon::run_daemon;
pub use repl::{run_repl, ReplConfig, ReplState};

// Interactive components
pub use interactive::{
    display_agent_status, display_tool_call, prompt_recovery, AgentModeSwitcher, AgentSwitchResult,
    MultiProgress, PermissionConfirm, PermissionResult, ProgressIndicator, RiskLevel,
    StreamingDisplay,
};

#[cfg(feature = "grpc")]
pub use grpc_client::{create_client, ClientConfig, ClientError, NdcClient};
