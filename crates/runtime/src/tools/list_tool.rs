//! List Tool - Directory listing
//!
//! Lists directory contents.
//! Design参考 OpenCode list.ts

use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;

use super::schema::ToolSchemaBuilder;
use super::{enforce_path_boundary, Tool, ToolError, ToolMetadata, ToolResult};

/// List tool - 列出目录内容
#[derive(Debug)]
pub struct ListTool;

impl Default for ListTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ListTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ListTool {
    fn name(&self) -> &str {
        "list"
    }

    fn description(&self) -> &str {
        "List directory contents. Returns a list of files and directories in the specified path."
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

        // Check if path is a directory
        if !path.exists() {
            return Err(ToolError::InvalidPath(path));
        }
        if !path.is_dir() {
            return Err(ToolError::InvalidArgument(format!(
                "'{}' is not a directory",
                path_str
            )));
        }

        enforce_path_boundary(path.as_path(), None, "list")?;

        let start = std::time::Instant::now();

        // Read directory
        let mut entries = fs::read_dir(&path).await.map_err(ToolError::Io)?;

        let mut dirs = Vec::new();
        let mut files = Vec::new();

        while let Some(entry) = entries.next_entry().await.map_err(ToolError::Io)? {
            let metadata = entry.metadata().await.map_err(ToolError::Io)?;

            let name = entry.file_name().to_string_lossy().into_owned();

            if metadata.is_dir() {
                dirs.push(format!("{}/", name));
            } else {
                files.push(name);
            }
        }

        // Sort: directories first, then files
        dirs.sort();
        files.sort();
        let items: Vec<String> = dirs.into_iter().chain(files).collect();

        let output = if items.is_empty() {
            "(empty)".to_string()
        } else {
            items.join("\n")
        };

        let duration = start.elapsed().as_millis() as u64;

        debug!("Listed {} items in {}", items.len(), path.display());

        Ok(ToolResult {
            success: true,
            output,
            error: None,
            metadata: ToolMetadata {
                execution_time_ms: duration,
                files_read: 0,
                files_written: 0,
                bytes_processed: 0,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        ToolSchemaBuilder::new()
            .description("List directory contents")
            .required_string("path", "The absolute path to the directory to list")
            .build()
            .to_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_list_directory() {
        let temp_dir = TempDir::new().unwrap();
        let _file1 = File::create(temp_dir.path().join("file1.txt")).unwrap();
        let _file2 = File::create(temp_dir.path().join("file2.txt")).unwrap();
        let _dir = std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let tool = ListTool::new();
        let params = serde_json::json!({
            "path": temp_dir.path().to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file2.txt"));
        assert!(result.output.contains("subdir/"));
    }

    #[tokio::test]
    async fn test_list_empty_directory() {
        let temp_dir = TempDir::new().unwrap();

        let tool = ListTool::new();
        let params = serde_json::json!({
            "path": temp_dir.path().to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output, "(empty)");
    }

    #[tokio::test]
    async fn test_list_missing_path() {
        let tool = ListTool::new();
        let params = serde_json::json!({});

        let result = tool.execute(&params).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(ToolError::InvalidArgument(_))));
    }

    #[tokio::test]
    async fn test_list_relative_path_error() {
        let tool = ListTool::new();
        let params = serde_json::json!({
            "path": "./relative/path"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }
}
