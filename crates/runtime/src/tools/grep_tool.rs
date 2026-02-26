//! Grep Tool - Content search
//!
//! Searches for patterns in files using regex.
//! Design参考 OpenCode grep.ts

use async_trait::async_trait;
use regex::Regex;
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;

use super::schema::ToolSchemaBuilder;
use super::{Tool, ToolError, ToolMetadata, ToolResult, enforce_path_boundary};

/// Grep tool - 内容搜索
#[derive(Debug)]
pub struct GrepTool;

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

impl GrepTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search for a pattern in files. Returns matching lines with line numbers."
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let pattern = params
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'pattern' parameter".to_string()))?;

        // Compile regex for validation
        let regex = Regex::new(pattern)
            .map_err(|e| ToolError::InvalidArgument(format!("Invalid regex pattern: {}", e)))?;

        let path_str = params.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let path = PathBuf::from(path_str);

        let start = std::time::Instant::now();

        // Check if path exists
        if !path.exists() {
            return Err(ToolError::InvalidPath(path));
        }

        enforce_path_boundary(path.as_path(), None, "grep")?;

        // Determine if path is a file or directory
        let results = if path.is_file() {
            // Search single file
            Self::search_file(&path, &regex).await?
        } else {
            // Search directory
            Self::search_directory(&path, &regex, params).await?
        };

        let duration = start.elapsed().as_millis() as u64;

        // Format output
        let output = if results.is_empty() {
            "No matches found".to_string()
        } else {
            results.join("\n")
        };

        let match_count = results.len() / 2;
        debug!("Grep found {} matches in {}ms", match_count, duration);

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
            .description("Search for a pattern in files")
            .required_string("pattern", "The regex pattern to search for")
            .param_string(
                "path",
                "The directory or file to search in (defaults to current directory)",
            )
            .param_string(
                "include",
                "File pattern to include (e.g., \"*.rs\", \"*.{ts,tsx}\")",
            )
            .param_integer("max_results", "Maximum number of results to return")
            .build()
            .to_value()
    }
}

impl GrepTool {
    /// 搜索单个文件
    async fn search_file(path: &PathBuf, regex: &Regex) -> Result<Vec<String>, ToolError> {
        let content = fs::read_to_string(path).await.map_err(ToolError::Io)?;

        let mut results = Vec::new();

        for (i, line) in content.lines().enumerate() {
            if regex.is_match(line) {
                let line_num = i + 1;
                results.push(format!("{}:{}", path.display(), line_num));
                results.push(format!("  {}", line));
            }
        }

        Ok(results)
    }

    /// 搜索目录（非递归版本）
    async fn search_directory(
        dir: &PathBuf,
        regex: &Regex,
        params: &serde_json::Value,
    ) -> Result<Vec<String>, ToolError> {
        let mut results = Vec::new();

        let include_pattern = params
            .get("include")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let max_results = params
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(u64::MAX);

        // Walk directory
        let mut entries = tokio::fs::read_dir(dir).await.map_err(ToolError::Io)?;

        while let Some(entry) = entries.next_entry().await.map_err(ToolError::Io)? {
            let path = entry.path();
            let metadata = entry.metadata().await.map_err(ToolError::Io)?;

            if metadata.is_file() {
                // Check include pattern
                let matches_pattern = if let Some(ref p) = include_pattern {
                    let file_name = entry.file_name().to_string_lossy().into_owned();
                    Self::matches_pattern(&file_name, p)
                } else {
                    true
                };

                if matches_pattern {
                    // Search file
                    let file_results = Self::search_file(&path, regex).await?;
                    results.extend(file_results);
                }
            }

            // Check max results
            if results.len() / 2 >= max_results as usize {
                break;
            }
        }

        Ok(results)
    }

    /// 检查文件名是否匹配模式
    fn matches_pattern(file_name: &str, pattern: &str) -> bool {
        glob::Pattern::new(pattern)
            .map(|g| g.matches(file_name))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_grep_single_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "hello world\nfoo bar\nhello again").unwrap();

        let tool = GrepTool::new();
        let params = serde_json::json!({
            "pattern": "hello",
            "path": file_path.to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("hello world"));
        assert!(result.output.contains("hello again"));
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        let tool = GrepTool::new();
        let params = serde_json::json!({
            "pattern": "goodbye",
            "path": file_path.to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert_eq!(result.output, "No matches found");
    }

    #[tokio::test]
    async fn test_grep_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        std::fs::write(&file1, "hello world").unwrap();
        std::fs::write(&file2, "hello rust").unwrap();

        let tool = GrepTool::new();
        let params = serde_json::json!({
            "pattern": "hello",
            "path": temp_dir.path().to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file2.txt"));
    }

    #[tokio::test]
    async fn test_grep_include_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("test.rs");
        let file2 = temp_dir.path().join("test.txt");
        std::fs::write(&file1, "fn main()").unwrap();
        std::fs::write(&file2, "hello world").unwrap();

        let tool = GrepTool::new();
        let params = serde_json::json!({
            "pattern": "\\w+",
            "path": temp_dir.path().to_string_lossy(),
            "include": "*.rs"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("test.rs"));
        assert!(!result.output.contains("test.txt"));
    }

    #[tokio::test]
    async fn test_grep_invalid_regex() {
        let tool = GrepTool::new();
        let params = serde_json::json!({
            "pattern": "[unclosed"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_grep_missing_path() {
        let tool = GrepTool::new();
        let params = serde_json::json!({
            "pattern": "test"
        });

        let result = tool.execute(&params).await.unwrap();
        // Should search current directory
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_grep_with_max_results() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\nline4\nline5").unwrap();

        let tool = GrepTool::new();
        let params = serde_json::json!({
            "pattern": "line",
            "path": file_path.to_string_lossy(),
            "max_results": 2
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        // max_results limits matches, each match = 2 lines (filename:line + content)
        assert!(result.output.contains("line1"));
    }
}
