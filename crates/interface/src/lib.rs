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

pub mod cli;
pub mod repl;
pub mod daemon;
pub mod agent_mode;
pub mod interactive;

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

pub use cli::{run, CliConfig};
pub use repl::{run_repl, ReplConfig, ReplState};
pub use daemon::run_daemon;
pub use agent_mode::{
    AgentModeManager, AgentModeConfig, AgentModeState,
    AgentModeStatus, PermissionRule,
    handle_agent_command, show_agent_status,
};

// Interactive components
pub use interactive::{
    StreamingDisplay, AgentModeSwitcher, AgentSwitchResult,
    PermissionConfirm, PermissionResult, ProgressIndicator, MultiProgress,
    prompt_recovery, display_agent_status, display_tool_call,
    RiskLevel,
};

#[cfg(feature = "grpc")]
pub use grpc_client::{NdcClient, ClientConfig, create_client, ClientError};
