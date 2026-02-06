//! GitTool - Git operations via shell
//!
//! Provides safe Git operations using shell commands
//! - Status check
//! - Branch operations
//! - Commit operations

use super::{Tool, ToolResult, ToolError, ToolContext};
use tracing::debug;
use tokio::process::Command;

/// Git tool using shell commands
#[derive(Debug)]
pub struct GitTool {
    #[allow(dead_code)]
    context: ToolContext,
}

impl GitTool {
    pub fn new() -> Self {
        Self {
            context: ToolContext::default(),
        }
    }

    async fn git(&self, args: &[&str]) -> Result<String, ToolError> {
        let mut cmd = Command::new("git");
        cmd.args(args);

        let output = cmd.output().await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            return Err(ToolError::ExecutionFailed(stderr.to_string()));
        }

        Ok(stdout.into_owned())
    }
}

#[async_trait::async_trait]
impl Tool for GitTool {
    fn name(&self) -> &str {
        "git"
    }

    fn description(&self) -> &str {
        "Git operations: status, branch, commit"
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let operation = params.get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing operation".to_string()))?;

        debug!("GitTool executing: {}", operation);

        let start = std::time::Instant::now();
        let (output, bytes) = match operation {
            "status" => {
                let out = self.git(&["status", "--porcelain"]).await?;
                (out.clone(), out.len())
            }
            "branch" => {
                let out = self.git(&["branch", "-a"]).await?;
                (out.clone(), out.len())
            }
            "branch_current" => {
                let out = self.git(&["rev-parse", "--abbrev-ref", "HEAD"]).await?;
                (out.clone(), out.len())
            }
            "log" => {
                let out = self.git(&["log", "--oneline", "-10"]).await?;
                (out.clone(), out.len())
            }
            "diff_staged" => {
                let out = self.git(&["diff", "--cached"]).await?;
                (out.clone(), out.len())
            }
            "diff" => {
                let out = self.git(&["diff"]).await?;
                (out.clone(), out.len())
            }
            "commit" => {
                let message = params.get("message")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArgument("Missing commit message".to_string()))?;
                let out = self.git(&["commit", "-m", message]).await?;
                (out.clone(), out.len())
            }
            "stash" => {
                let out = self.git(&["stash", "push", "-m", "auto-stash"]).await?;
                (out.clone(), out.len())
            }
            "stash_pop" => {
                let out = self.git(&["stash", "pop"]).await?;
                (out.clone(), out.len())
            }
            "remote" => {
                let out = self.git(&["remote", "-v"]).await?;
                (out.clone(), out.len())
            }
            "fetch" => {
                let out = self.git(&["fetch"]).await?;
                (out.clone(), out.len())
            }
            _ => return Err(ToolError::InvalidArgument(format!("Unknown git operation: {}", operation)))
        };

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            success: true,
            output,
            error: None,
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
                "operation": {
                    "type": "string",
                    "enum": ["status", "branch", "branch_current", "log", "diff_staged", "diff", "commit", "stash", "stash_pop", "remote", "fetch"],
                    "description": "Git operation"
                },
                "message": {
                    "type": "string",
                    "description": "Commit message for commit operation"
                }
            },
            "required": ["operation"]
        })
    }
}
