//! Write Tool - File writing
//!
//! Writes content to a file, with optional create intermediate directories.
//! Design参考 OpenCode write.ts

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;

use super::schema::ToolSchemaBuilder;
use super::{Tool, ToolError, ToolMetadata, ToolResult, enforce_path_boundary};

/// Atomic file write: writes to a temporary file then renames to the target path.
/// Prevents data corruption if the process crashes during write.
pub async fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content).await?;
    fs::rename(&tmp, path).await?;
    Ok(())
}

/// Write tool - 写入文件
#[derive(Debug)]
pub struct WriteTool;

impl Default for WriteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WriteTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, or overwrites it if it does. Use 'edit' for modifying existing files."
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let path_str = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'path' parameter".to_string()))?;

        // Validate path is absolute
        let path = PathBuf::from(path_str);
        if !path.is_absolute() {
            return Err(ToolError::InvalidArgument(
                "path must be an absolute path, not relative".to_string(),
            ));
        }

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'content' parameter".to_string()))?;

        enforce_path_boundary(path.as_path(), None, "write")?;

        let start = std::time::Instant::now();

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent).await.map_err(ToolError::Io)?;
        }

        // Check if file exists and handle mode
        let append = params
            .get("append")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mode = if append { "appended to" } else { "written to" };

        if append && path.exists() {
            // Append to existing file (atomic: read + concat + write-tmp + rename)
            let existing = fs::read_to_string(&path).await.map_err(ToolError::Io)?;
            let new_content = existing + content;
            atomic_write(&path, &new_content)
                .await
                .map_err(ToolError::Io)?;
        } else {
            // Write (or create) file atomically
            atomic_write(&path, content)
                .await
                .map_err(ToolError::Io)?;
        }

        let bytes_written = content.len();
        let duration = start.elapsed().as_millis() as u64;

        debug!("{} {} bytes to {}", mode, bytes_written, path.display());

        Ok(ToolResult {
            success: true,
            output: format!("Wrote {} bytes to {}", bytes_written, path.display()),
            error: None,
            metadata: ToolMetadata {
                execution_time_ms: duration,
                files_read: 0,
                files_written: 1,
                bytes_processed: bytes_written as u64,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        ToolSchemaBuilder::new()
            .description("Write content to a file")
            .required_string("path", "The absolute path to the file to write")
            .required_string("content", "The content to write to the file")
            .param_boolean(
                "append",
                "Whether to append to existing file instead of overwriting",
            )
            .build()
            .to_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_write_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let tool = WriteTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "content": "Hello, World!"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("13 bytes"));

        // Verify file was written
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello, World!");
        assert_eq!(result.metadata.files_written, 1);
    }

    #[tokio::test]
    async fn test_write_creates_directories() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nested").join("deep").join("test.txt");

        let tool = WriteTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "content": "nested content"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(file_path.exists());
    }

    #[tokio::test]
    async fn test_append_to_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "original").unwrap();

        let tool = WriteTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "content": " appended",
            "append": true
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "original appended");
    }

    #[tokio::test]
    async fn test_overwrite_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "original").unwrap();

        let tool = WriteTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "content": "new content"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "new content");
    }

    #[tokio::test]
    async fn test_write_missing_path() {
        let tool = WriteTool::new();
        let params = serde_json::json!({
            "content": "test"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_missing_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let tool = WriteTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy()
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_relative_path_error() {
        let tool = WriteTool::new();
        let params = serde_json::json!({
            "path": "./relative/path.txt",
            "content": "test"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_is_atomic_no_tmp_leftover() {
        // After a successful write, no .tmp file should remain
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("atomic.txt");

        let tool = WriteTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "content": "atomic content"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);

        // File should exist with correct content
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "atomic content");

        // No .tmp file should remain
        let tmp_path = file_path.with_extension("tmp");
        assert!(!tmp_path.exists(), ".tmp file should not remain after atomic write");
    }

    #[tokio::test]
    async fn test_write_atomic_preserves_content_on_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("overwrite.txt");
        std::fs::write(&file_path, "original content").unwrap();

        let tool = WriteTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "content": "new content via atomic write"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "new content via atomic write");
    }

    #[tokio::test]
    async fn test_append_atomic_no_tmp_leftover() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("append.txt");
        std::fs::write(&file_path, "base").unwrap();

        let tool = WriteTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "content": " appended",
            "append": true
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "base appended");

        let tmp_path = file_path.with_extension("tmp");
        assert!(!tmp_path.exists(), ".tmp file should not remain after append");
    }

    #[tokio::test]
    async fn test_atomic_write_helper_basic() {
        use super::atomic_write;
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("helper_test.txt");

        atomic_write(&file_path, "hello atomic").await.unwrap();
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello atomic");

        // No .tmp file should remain
        let tmp_path = file_path.with_extension("tmp");
        assert!(!tmp_path.exists());
    }

    #[tokio::test]
    async fn test_atomic_write_helper_overwrites_existing() {
        use super::atomic_write;
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("overwrite.txt");
        std::fs::write(&file_path, "original").unwrap();

        atomic_write(&file_path, "replaced").await.unwrap();
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "replaced");
    }
}
