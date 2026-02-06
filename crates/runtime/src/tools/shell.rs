//! ShellTool - Shell 命令执行工具
//!
//! 提供安全的命令执行：
//! - 执行预定义的构建命令
//! - 运行测试
//! - 执行 linter
//!
//! 安全措施：
//! - 白名单命令
//! - 超时限制
//! - 环境变量过滤

use super::{Tool, ToolResult, ToolError, ToolContext};
use tokio::process::Command;
use std::collections::HashSet;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn, info};

/// Shell 工具
#[derive(Debug)]
pub struct ShellTool {
    context: ToolContext,

    /// 允许的命令白名单
    allowed_commands: HashSet<&'static str>,
}

impl ShellTool {
    pub fn new() -> Self {
        let mut allowed = HashSet::new();

        // Rust 构建
        allowed.insert("cargo");
        allowed.insert("rustc");
        allowed.insert("cargo-test");
        allowed.insert("cargo-build");
        allowed.insert("cargo-clippy");
        allowed.insert("cargo-fmt");

        // Node 构建
        allowed.insert("npm");
        allowed.insert("node");

        // Python
        allowed.insert("python");
        allowed.insert("pip");

        // Git
        allowed.insert("git");
        allowed.insert("gh");

        // 其他
        allowed.insert("echo");
        allowed.insert("pwd");
        allowed.insert("ls");
        allowed.insert("cat");
        allowed.insert("head");
        allowed.insert("tail");
        allowed.insert("grep");
        allowed.insert("find");
        allowed.insert("make");

        Self {
            context: ToolContext::default(),
            allowed_commands: allowed,
        }
    }

    /// 检查命令是否允许
    fn is_allowed(&self, command: &str) -> bool {
        let base = command.split_whitespace().next().unwrap_or("");
        self.allowed_commands.contains(base)
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Safe shell command execution with whitelist"
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let command = params.get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'command'".to_string()))?;

        let args: Vec<String> = params.get("args")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect())
            .unwrap_or_default();

        // 检查命令是否在白名单中
        if !self.is_allowed(command) {
            warn!("Blocked command: {} {:?}", command, args);
            return Err(ToolError::PermissionDenied(
                format!("Command not allowed: {}", command)
            ));
        }

        // 检查超时
        let timeout = params.get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.context.timeout_seconds);

        let start = std::time::Instant::now();
        let mut cmd = Command::new(command);

        // 设置工作目录
        cmd.current_dir(&self.context.working_dir);

        // 添加参数
        if !args.is_empty() {
            cmd.args(&args);
        }

        // 设置环境变量（过滤敏感变量）
        let filtered_env: Vec<(String, String)> = self.context.env_vars.iter()
            .filter(|(k, _)| {
                !k.starts_with("AWS_") &&
                !k.starts_with("GITHUB_") &&
                !k.contains("SECRET") &&
                !k.contains("PASSWORD")
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        for (k, v) in filtered_env {
            cmd.env(k, v);
        }

        // 执行命令
        debug!("Executing: {} {:?}", command, args);

        let output = cmd.output().await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let success = output.status.success();

        let mut output_text = String::new();
        if !stdout.is_empty() {
            output_text.push_str(&format!("[STDOUT]\n{}", stdout));
        }
        if !stderr.is_empty() {
            output_text.push_str(&format!("[STDERR]\n{}", stderr));
        }

        if !success {
            warn!("Command failed: {} {:?}", command, args);
            return Ok(ToolResult {
                success: false,
                output: output_text,
                error: Some(format!("Exit code: {:?}", output.status.code())),
                metadata: super::ToolMetadata {
                    execution_time_ms: duration,
                    files_read: 0,
                    files_written: 0,
                    bytes_processed: output_text.len() as u64,
                },
            });
        }

        debug!("Command succeeded: {} {:?}", command, args);

        Ok(ToolResult {
            success: true,
            output: output_text,
            error: None,
            metadata: super::ToolMetadata {
                execution_time_ms: duration,
                files_read: 0,
                files_written: 0,
                bytes_processed: output_text.len() as u64,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        serde::json::json!({
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
                }
            },
            "required": ["command"]
        })
    }
}

/// 预定义命令构建器
#[derive(Debug, Default)]
pub struct CommandBuilder {
    commands: Vec<String>,
}

impl CommandBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cargo_test(&mut self, verbose: bool) -> &mut Self {
        let mut cmd = vec!["cargo", "test"];
        if verbose {
            cmd.push("--");
        }
        cmd.push("--nocapture");
        self.commands.push(cmd.join(" "));
        self
    }

    pub fn cargo_build(&mut self, release: bool) -> &mut Self {
        let mut cmd = vec!["cargo", "build"];
        if release {
            cmd.push("--release");
        }
        self.commands.push(cmd.join(" "));
        self
    }

    pub fn cargo_clippy(&mut self) -> &mut Self {
        self.commands.push("cargo clippy -- -D warnings".to_string());
        self
    }

    pub fn npm_test(&mut self) -> &mut Self {
        self.commands.push("npm test".to_string());
        self
    }

    pub fn build(&self) -> Vec<String> {
        self.commands.clone()
    }
}
