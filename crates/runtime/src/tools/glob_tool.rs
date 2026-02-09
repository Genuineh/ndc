//! Glob Tool - File pattern matching
//!
//! Finds files matching glob patterns.
//! Design参考 OpenCode glob.ts

use async_trait::async_trait;
use std::path::PathBuf;
use glob::glob;
use tracing::debug;

use super::{Tool, ToolResult, ToolError, ToolMetadata};
use super::schema::ToolSchemaBuilder;

/// Glob tool - 文件模式匹配
#[derive(Debug)]
pub struct GlobTool;

impl GlobTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching glob patterns. Supports recursive search with **/* patterns."
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let pattern = params.get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'pattern' parameter".to_string()))?;

        let path_str = params.get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let path = PathBuf::from(path_str);
        let base_dir = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .map_err(|e| ToolError::Io(e))?
                .join(path)
        };

        let start = std::time::Instant::now();

        // Build full pattern
        let full_pattern = if pattern.starts_with('/') {
            // Absolute pattern
            format!("{}", pattern)
        } else {
            // Relative to path
            format!("{}/{}", base_dir.display(), pattern)
        };

        // Execute glob
        let mut matches = Vec::new();
        let mut directories = Vec::new();
        let mut files = Vec::new();

        for entry in glob(&full_pattern)
            .map_err(|e| ToolError::InvalidArgument(format!("Invalid glob pattern: {}", e)))? {

            match entry {
                Ok(path) => {
                    if path.is_dir() {
                        directories.push(path.display().to_string());
                    } else {
                        files.push(path.display().to_string());
                    }
                    matches.push(path.display().to_string());
                }
                Err(e) => {
                    tracing::debug!("Glob error for {}: {}", full_pattern, e);
                }
            }
        }

        // Sort results
        directories.sort();
        files.sort();
        let mut results = directories;
        results.extend(files);

        let duration = start.elapsed().as_millis() as u64;

        // Format output
        let output = if results.is_empty() {
            "No matches found".to_string()
        } else {
            results.join("\n")
        };

        let bytes = output.len();
        debug!("Glob found {} matches for pattern '{}'", results.len(), full_pattern);

        Ok(ToolResult {
            success: true,
            output,
            error: None,
            metadata: ToolMetadata {
                execution_time_ms: duration,
                files_read: 0,
                files_written: 0,
                bytes_processed: bytes as u64,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        ToolSchemaBuilder::new()
            .description("Find files matching glob patterns")
            .required_string("pattern", "The glob pattern (e.g., \"**/*.rs\", \"src/**/*.ts\")")
            .param_string("path", "Base directory for search (defaults to current directory)")
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
    async fn test_glob_single_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "test content").unwrap();

        let tool = GlobTool::new();
        let params = serde_json::json!({
            "pattern": "*.txt",
            "path": temp_dir.path().to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("test.txt"));
    }

    #[tokio::test]
    async fn test_glob_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let nested = temp_dir.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("deep.txt"), "deep").unwrap();
        std::fs::write(temp_dir.path().join("root.txt"), "root").unwrap();

        let tool = GlobTool::new();
        let params = serde_json::json!({
            "pattern": "**/*.txt",
            "path": temp_dir.path().to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("root.txt"));
        assert!(result.output.contains("deep.txt"));
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let temp_dir = TempDir::new().unwrap();

        let tool = GlobTool::new();
        let params = serde_json::json!({
            "pattern": "*.nonexistent",
            "path": temp_dir.path().to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert_eq!(result.output, "No matches found");
    }

    #[tokio::test]
    async fn test_glob_multiple_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let _f1 = File::create(temp_dir.path().join("test.rs")).unwrap();
        let _f2 = File::create(temp_dir.path().join("test.ts")).unwrap();
        let _f3 = File::create(temp_dir.path().join("test.js")).unwrap();

        let tool = GlobTool::new();
        let params = serde_json::json!({
            "pattern": "*.rs",
            "path": temp_dir.path().to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("test.rs"));
        assert!(!result.output.contains("test.ts"));
        assert!(!result.output.contains("test.js"));
    }

    #[tokio::test]
    async fn test_glob_missing_pattern() {
        let tool = GlobTool::new();
        let params = serde_json::json!({
            "path": "."
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_glob_default_path() {
        let tool = GlobTool::new();
        let params = serde_json::json!({
            "pattern": "*.rs"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
    }
}
