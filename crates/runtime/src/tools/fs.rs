//! FsTool - File system operations
//!
//! Provides safe file operations:
//! - Read files
//! - Write files
//! - Create files/directories
//! - Delete files
//! - List directory contents

use super::{enforce_path_boundary, Tool, ToolContext, ToolError, ToolResult};
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;

/// File system tool
#[derive(Debug)]
pub struct FsTool {
    context: ToolContext,
}

impl Default for FsTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FsTool {
    pub fn new() -> Self {
        Self {
            context: ToolContext::default(),
        }
    }
}

#[async_trait::async_trait]
impl Tool for FsTool {
    fn name(&self) -> &str {
        "fs"
    }

    fn description(&self) -> &str {
        "File system operations: read, write, create, delete, list"
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let operation = params
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing operation".to_string()))?;

        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .ok_or_else(|| ToolError::InvalidArgument("Missing path".to_string()))?;
        let working_dir = params
            .get("working_dir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from);

        enforce_path_boundary(
            path.as_path(),
            working_dir
                .as_deref()
                .or(Some(self.context.working_dir.as_path())),
            format!("fs:{}", operation).as_str(),
        )?;

        debug!("FsTool executing: {} on {}", operation, path.display());

        let start = std::time::Instant::now();
        let mut files_read = 0u32;
        let mut files_written = 0u32;

        let output = match operation {
            "read" => {
                let content = fs::read_to_string(&path)
                    .await
                    .map_err(ToolError::Io)?;
                files_read = 1;
                content
            }
            "write" => {
                let content = params
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArgument("Missing content".to_string()))?;
                fs::write(&path, content)
                    .await
                    .map_err(ToolError::Io)?;
                files_written = 1;
                format!("Written {} bytes to {}", content.len(), path.display())
            }
            "create" => {
                if path.extension().is_some() {
                    fs::write(&path, "").await.map_err(ToolError::Io)?;
                } else {
                    fs::create_dir_all(&path)
                        .await
                        .map_err(ToolError::Io)?;
                }
                files_written = 1;
                format!("Created {}", path.display())
            }
            "delete" => {
                if path.is_file() {
                    fs::remove_file(&path).await.map_err(ToolError::Io)?;
                } else {
                    fs::remove_dir_all(&path)
                        .await
                        .map_err(ToolError::Io)?;
                }
                format!("Deleted {}", path.display())
            }
            "list" => {
                let mut entries = tokio::fs::read_dir(&path)
                    .await
                    .map_err(ToolError::Io)?;
                let mut items = Vec::new();
                while let Some(entry) = entries.next_entry().await.map_err(ToolError::Io)? {
                    items.push(entry.file_name().to_string_lossy().into_owned());
                }
                items.join("\n")
            }
            "exists" => if path.exists() { "true" } else { "false" }.to_string(),
            _ => {
                return Err(ToolError::InvalidArgument(format!(
                    "Unknown operation: {}",
                    operation
                )));
            }
        };

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            success: true,
            output,
            error: None,
            metadata: super::ToolMetadata {
                execution_time_ms: duration,
                files_read,
                files_written,
                bytes_processed: 0,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["read", "write", "create", "delete", "list", "exists"],
                    "description": "File operation type"
                },
                "path": {
                    "type": "string",
                    "description": "File path"
                },
                "content": {
                    "type": "string",
                    "description": "File content for write"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Optional project root/working directory used for boundary checks"
                }
            },
            "required": ["operation", "path"]
        })
    }
}
