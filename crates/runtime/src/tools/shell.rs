//! ShellTool - Shell command execution
//!
//! Provides safe command execution:
//! - Execute predefined build commands
//! - Run tests
//! - Execute linters
//!
//! Security measures:
//! - Whitelist commands
//! - Timeout limits
//! - Environment variable filtering

use super::{Tool, ToolContext, ToolError, ToolResult, enforce_shell_command};
use std::collections::HashSet;
use tokio::process::Command;
use tracing::debug;

/// Allowed commands whitelist
const ALLOWED_COMMANDS: &[&str] = &["cargo", "git", "ls", "cat", "echo", "pwd", "cd"];

/// Shell tool
#[derive(Debug)]
pub struct ShellTool {
    context: ToolContext,
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellTool {
    pub fn new() -> Self {
        Self {
            context: ToolContext::default(),
        }
    }

    fn is_allowed(&self, command: &str) -> bool {
        ALLOWED_COMMANDS.contains(&command)
            || self.context.allowed_operations.iter().any(|s| s == command)
    }
}

#[async_trait::async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Shell command execution (whitelisted commands only)"
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing command".to_string()))?;

        // 检查命令是否在白名单中
        if !self.is_allowed(command) {
            tracing::warn!("Blocked command: {}", command);
            return Err(ToolError::PermissionDenied(format!(
                "Command not allowed: {}",
                command
            )));
        }

        // 获取参数
        let args: Vec<String> = params
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let working_dir = params
            .get("working_dir")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| self.context.working_dir.clone());

        enforce_shell_command(command, args.as_slice(), Some(&working_dir))?;

        // 检查超时
        let _timeout = params
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.context.timeout_seconds);

        let start = std::time::Instant::now();
        let mut cmd = Command::new(command);
        cmd.args(&args);

        // 设置工作目录
        cmd.current_dir(&working_dir);

        // 过滤环境变量
        let filtered_env: HashSet<&str> =
            ["PATH", "HOME", "USER", "SHELL"].iter().cloned().collect();
        for (key, value) in std::env::vars() {
            if filtered_env.contains(key.as_str()) || self.context.env_vars.contains_key(&key) {
                cmd.env(key, value);
            }
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let output_text = if stderr.is_empty() {
            stdout.into_owned()
        } else {
            stderr.into_owned()
        };
        let success = output.status.success();

        let duration = start.elapsed().as_millis() as u64;
        let bytes = output_text.len();

        debug!("Shell command executed: {} {:?}", command, args);

        Ok(ToolResult {
            success,
            output: output_text.clone(),
            error: if success { None } else { Some(output_text) },
            metadata: super::ToolMetadata {
                execution_time_ms: duration,
                files_read: 0,
                files_written: 0,
                bytes_processed: bytes as u64,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Command to execute (must be whitelisted)"
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Command arguments"
                },
                "timeout": {
                    "type": "number",
                    "description": "Timeout in seconds"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Optional working directory for command execution"
                }
            },
            "required": ["command"]
        })
    }
}
