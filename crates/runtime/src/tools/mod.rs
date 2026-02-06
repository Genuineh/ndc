//! Tools - Controlled toolset
//!
//! Responsibilities:
//! - Safe file operations
//! - Safe Git operations
//! - Safe Shell command execution
//! - All operations validated and logged

mod trait_mod;
pub use trait_mod::{Tool, ToolResult, ToolError, ToolContext, ToolManager, ToolMetadata};

pub mod fs;
pub use fs::FsTool;

pub mod git;
pub use git::GitTool;

pub mod shell;
pub use shell::ShellTool;
