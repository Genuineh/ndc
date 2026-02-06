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

#[cfg(feature = "grpc")]
pub mod generated;

#[cfg(feature = "grpc")]
pub mod grpc_client;

#[cfg(test)]
mod cli_tests;

#[cfg(test)]
mod daemon_tests;

#[cfg(test)]
mod e2e_tests;

pub use cli::{run_cli, CliConfig};
pub use repl::{run_repl, ReplConfig, ReplState};
pub use daemon::run_daemon;

#[cfg(feature = "grpc")]
pub use grpc_client::{NdcClient, ClientConfig, create_client, ClientError};
