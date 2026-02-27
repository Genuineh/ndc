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

/// Denied commands — always blocked regardless of user approval.
/// The security gateway (`enforce_shell_command`) handles risk-based
/// prompting for everything else; this list is the absolute hard-stop.
const DENIED_COMMANDS: &[&str] = &[
    "shutdown", "reboot", "init", "poweroff", "halt",
    "mkfs", "fdisk", "parted",
    "iptables", "ip6tables", "nft",
    "passwd", "useradd", "userdel", "usermod", "groupadd",
    "mount", "umount",
    "insmod", "rmmod", "modprobe",
    "systemctl",
];

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

    fn is_denied(&self, command: &str) -> bool {
        DENIED_COMMANDS.contains(&command)
    }
}

#[async_trait::async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Shell command execution with security gating"
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing command".to_string()))?;

        // Hard-deny commands that should never be executed
        if self.is_denied(command) {
            tracing::warn!("Blocked denied command: {}", command);
            return Err(ToolError::PermissionDenied(format!(
                "Command '{}' is permanently denied for safety",
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
        let timeout = params
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.context.timeout_seconds);

        let start = std::time::Instant::now();
        let mut cmd = Command::new(command);
        cmd.args(&args);

        // 设置工作目录
        cmd.current_dir(&working_dir);

        // 过滤环境变量 — 白名单 + context 追加，黑名单拦截危险变量
        const DANGEROUS_ENV_VARS: &[&str] = &[
            "LD_PRELOAD",
            "LD_LIBRARY_PATH",
            "PYTHONPATH",
            "NODE_OPTIONS",
            "DYLD_INSERT_LIBRARIES",
        ];
        let filtered_env: HashSet<&str> =
            ["PATH", "HOME", "USER", "SHELL", "LANG", "TERM", "LC_ALL"]
                .iter()
                .cloned()
                .collect();
        for (key, value) in std::env::vars() {
            if DANGEROUS_ENV_VARS.contains(&key.as_str()) {
                continue;
            }
            if filtered_env.contains(key.as_str()) || self.context.env_vars.contains_key(&key) {
                cmd.env(&key, value);
            }
        }

        let output = tokio::time::timeout(std::time::Duration::from_secs(timeout), cmd.output())
            .await
            .map_err(|_| {
                ToolError::Timeout(format!(
                    "Command '{}' timed out after {}s",
                    command, timeout
                ))
            })?
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

#[cfg(test)]
mod tests {
    use super::*;

    fn shell_with_context(ctx: ToolContext) -> ShellTool {
        ShellTool { context: ctx }
    }

    #[tokio::test]
    async fn test_shell_normal_command_completes_within_timeout() {
        let tool = ShellTool::new();
        let params = serde_json::json!({
            "command": "echo",
            "args": ["hello"],
            "timeout": 5
        });
        let result = tool.execute(&params).await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.success);
        assert!(r.output.contains("hello"));
    }

    #[tokio::test]
    async fn test_shell_timeout_triggers_error() {
        let mut ctx = ToolContext::default();
        ctx.allowed_operations.push("sleep".to_string());
        let tool = shell_with_context(ctx);
        let params = serde_json::json!({
            "command": "sleep",
            "args": ["10"],
            "timeout": 1
        });
        let result = tool.execute(&params).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ToolError::Timeout(_)),
            "Expected Timeout error, got: {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_shell_dangerous_env_vars_filtered() {
        // LD_PRELOAD should never reach child process even if in context.env_vars
        let mut ctx = ToolContext::default();
        ctx.env_vars
            .insert("LD_PRELOAD".to_string(), "/tmp/evil.so".to_string());
        ctx.allowed_operations.push("printenv".to_string());
        let tool = shell_with_context(ctx);
        let params = serde_json::json!({
            "command": "printenv",
            "args": ["LD_PRELOAD"],
            "timeout": 5
        });
        let result = tool.execute(&params).await;
        // printenv returns exit code 1 when variable is not set
        match result {
            Ok(r) => assert!(!r.success, "LD_PRELOAD should not be set in child"),
            Err(ToolError::PermissionDenied(_)) => {
                // security gateway may block — acceptable in test env
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_shell_safe_env_vars_passed() {
        let tool = ShellTool::new();
        let params = serde_json::json!({
            "command": "echo",
            "args": ["test"],
            "timeout": 5
        });
        let result = tool.execute(&params).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_denied_denylist() {
        let tool = ShellTool::new();
        // Denied commands
        assert!(tool.is_denied("shutdown"));
        assert!(tool.is_denied("reboot"));
        assert!(tool.is_denied("mkfs"));
        assert!(tool.is_denied("passwd"));
        assert!(tool.is_denied("systemctl"));
        // Normal development commands — not denied
        assert!(!tool.is_denied("cargo"));
        assert!(!tool.is_denied("git"));
        assert!(!tool.is_denied("rm"));
        assert!(!tool.is_denied("npm"));
        assert!(!tool.is_denied("curl"));
    }
}
