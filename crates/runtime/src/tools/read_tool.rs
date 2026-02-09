//! Read Tool - File reading with offset/limit
//!
//! Reads file contents with optional line offset and limit.
//! Design参考 OpenCode read.ts

use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;

use super::{Tool, ToolResult, ToolError, ToolMetadata};
use super::schema::ToolSchemaBuilder;

/// Read tool - 读取文件内容
#[derive(Debug)]
pub struct ReadTool;

impl ReadTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Supports optional offset and limit for large files."
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let path_str = params.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'path' parameter".to_string()))?;

        // Validate path is absolute
        let path = PathBuf::from(path_str);
        if !path.is_absolute() {
            return Err(ToolError::InvalidArgument(
                "path must be an absolute path, not relative".to_string()
            ));
        }

        // Check if file exists
        if !path.exists() {
            return Err(ToolError::InvalidPath(path));
        }
        if !path.is_file() {
            return Err(ToolError::InvalidArgument(
                format!("'{}' is not a file", path_str)
            ));
        }

        let start = std::time::Instant::now();

        // Read entire file first
        let content = fs::read_to_string(&path).await
            .map_err(|e| ToolError::Io(e))?;

        let total_lines = content.lines().count();
        let total_bytes = content.len();

        // Apply offset if provided
        let lines: Vec<&str> = if let Some(offset) = params.get("offset").and_then(|v| v.as_u64()) {
            let offset = offset as usize;
            if offset >= total_lines {
                return Err(ToolError::InvalidArgument(
                    format!("offset {} is beyond file length {}", offset, total_lines)
                ));
            }
            content.lines().skip(offset).collect()
        } else {
            content.lines().collect()
        };

        // Apply limit if provided
        let lines: Vec<&str> = if let Some(limit) = params.get("limit").and_then(|v| v.as_u64()) {
            let limit = limit as usize;
            let start_idx = 0;
            let end_idx = std::cmp::min(limit, lines.len() - start_idx);
            if start_idx >= lines.len() {
                Vec::new()
            } else {
                lines[start_idx..start_idx + end_idx].to_vec()
            }
        } else {
            lines
        };

        let displayed_lines = lines.len();
        let output = lines.join("\n");

        // Add line numbers if requested
        if params.get("number").and_then(|v| v.as_bool()).unwrap_or(false) {
            let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
            let numbered: Vec<String> = lines.iter().enumerate()
                .map(|(i, line)| format!("{:6}  {}", offset + i as u64 + 1, line))
                .collect();
            let output = numbered.join("\n");
            let duration = start.elapsed().as_millis() as u64;

            return Ok(ToolResult {
                success: true,
                output,
                error: None,
                metadata: ToolMetadata {
                    execution_time_ms: duration,
                    files_read: 1,
                    files_written: 0,
                    bytes_processed: total_bytes as u64,
                },
            });
        }

        let duration = start.elapsed().as_millis() as u64;

        debug!("Read {} bytes ({} lines, displayed {}) from {}",
               total_bytes, total_lines, displayed_lines, path.display());

        Ok(ToolResult {
            success: true,
            output,
            error: None,
            metadata: ToolMetadata {
                execution_time_ms: duration,
                files_read: 1,
                files_written: 0,
                bytes_processed: total_bytes as u64,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        ToolSchemaBuilder::new()
            .description("Read file contents")
            .required_string("path", "The absolute path to the file to read")
            .param_integer("offset", "Line number to start reading from (0-indexed)")
            .param_integer("limit", "Maximum number of lines to read")
            .param_boolean("number", "Whether to include line numbers")
            .build()
            .to_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs::File;
    use std::io::Write;

    #[tokio::test]
    async fn test_read_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = ReadTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("line1"));
        assert!(result.output.contains("line2"));
        assert!(result.output.contains("line3"));
        assert_eq!(result.metadata.files_read, 1);
    }

    #[tokio::test]
    async fn test_read_with_offset() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\nline4").unwrap();

        let tool = ReadTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "offset": 1
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("line2"));
        assert!(result.output.contains("line3"));
        assert!(!result.output.contains("line1"));
    }

    #[tokio::test]
    async fn test_read_with_limit() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\nline4\nline5").unwrap();

        let tool = ReadTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "limit": 2
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output.lines().count(), 2);
    }

    #[tokio::test]
    async fn test_read_with_offset_and_limit() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\nline4\nline5").unwrap();

        let tool = ReadTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "offset": 1,
            "limit": 2
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        let lines: Vec<&str> = result.output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "line2");
        assert_eq!(lines[1], "line3");
    }

    #[tokio::test]
    async fn test_read_with_numbering() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = ReadTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "number": true
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("1  line1"));
        assert!(result.output.contains("2  line2"));
    }

    #[tokio::test]
    async fn test_read_missing_path() {
        let tool = ReadTool::new();
        let params = serde_json::json!({});

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_relative_path_error() {
        let tool = ReadTool::new();
        let params = serde_json::json!({
            "path": "./relative/path.txt"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }
}
