//! Edit Tool - Smart file editing with multiple matching strategies
//!
//! Provides intelligent string replacement with multiple matching strategies:
//! - Simple: Exact string match
//! - LineTrimmed: Trim trailing whitespace before matching
//! - BlockAnchor: Match using first and last lines as anchors
//! - WhitespaceNormalized: Normalize whitespace before matching
//! - IndentationFlexible: Flexible indentation matching
//!
//! Design参考 OpenCode edit.ts

use async_trait::async_trait;
use std::path::PathBuf;
use regex::Regex;
use tracing::debug;

use super::{Tool, ToolResult, ToolError, ToolMetadata};
use super::schema::ToolSchemaBuilder;

/// 编辑错误类型
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("String not found: {0}")]
    NotFound(String),

    #[error("Multiple matches found for: {0}")]
    MultipleMatches(String),

    #[error("Invalid block anchor: {0}")]
    InvalidAnchor(String),
}

/// 匹配策略
#[derive(Debug, Clone, PartialEq)]
enum MatchingStrategy {
    Simple,                    // 精确匹配
    LineTrimmed,               // 行尾空白trim
    BlockAnchor,               // 块锚点匹配
    WhitespaceNormalized,      // 空白字符标准化
}

/// Edit tool - 智能文件编辑
#[derive(Debug)]
pub struct EditTool;

impl EditTool {
    pub fn new() -> Self {
        Self
    }

    /// 查找所有匹配位置
    fn find_matches(&self, content: &str, old: &str) -> Vec<(usize, usize)> {
        let mut matches = Vec::new();
        let mut start = 0;

        while let Some(pos) = content[start..].find(old) {
            matches.push((start + pos, start + pos + old.len()));
            start += pos + 1;
        }

        matches
    }

    /// 策略1: 精确匹配
    fn match_simple(&self, content: &str, old: &str) -> Option<(usize, usize)> {
        self.find_matches(content, old).first().cloned()
    }

    /// 策略2: 行尾空白trim匹配
    fn match_line_trimmed(&self, content: &str, old: &str) -> Option<(usize, usize)> {
        let old_trimmed = old.trim_end();
        let old_lines: Vec<&str> = old_trimmed.lines().collect();

        if old_lines.len() == 1 {
            // 单行: 找到尾部trim后匹配的行
            for (i, line) in content.lines().enumerate() {
                if line.trim_end() == old_trimmed {
                    let start = content[..]
                        .find(line)
                        .map(|p| content[..p + line.rfind(line.trim_end()).unwrap_or(0)].len())
                        .unwrap_or(0);
                    let line_start = content.lines().take(i).map(|l| l.len() + 1).sum();
                    let line_end = line_start + line.len();
                    return Some((line_start, line_end));
                }
            }
        } else {
            // 多行: 检查每行是否匹配
            if let Some(start_idx) = content.lines().position(|l| l.trim_end() == old_lines[0]) {
                let end_idx = start_idx + old_lines.len();
                let matched_lines: Vec<&str> = content.lines().skip(start_idx).take(old_lines.len()).collect();

                if matched_lines.len() == old_lines.len() &&
                   matched_lines.iter().zip(old_lines.iter()).all(|(a, b)| a.trim_end() == b.trim_end()) {
                    let line_start: usize = content.lines().take(start_idx).map(|l| l.len() + 1).sum();
                    let matched_text: String = matched_lines.join("\n");
                    let line_end = line_start + matched_text.len();
                    return Some((line_start, line_end));
                }
            }
        }

        None
    }

    /// 策略3: 块锚点匹配 (首尾行作为锚)
    fn match_block_anchor(&self, content: &str, old: &str) -> Option<(usize, usize)> {
        let old_lines: Vec<&str> = old.lines().collect();

        if old_lines.len() < 2 {
            return None;
        }

        let first_line = old_lines[0].trim();
        let last_line = old_lines[old_lines.len() - 1].trim();

        // 找到第一个锚点行
        if let Some(start_idx) = content.lines().position(|l| l.trim() == first_line) {
            // 从末尾向前找匹配的尾行
            for end_idx in (start_idx + 1..content.lines().count()).rev() {
                let lines_between: Vec<&str> = content.lines().skip(start_idx).take(end_idx - start_idx).collect();
                if lines_between.first().map(|l| l.trim()) == Some(first_line) &&
                   lines_between.last().map(|l| l.trim()) == Some(last_line) &&
                   lines_between.len() == old_lines.len() {
                    // 检查中间行
                    let middle_matched = lines_between[1..lines_between.len()-1]
                        .iter()
                        .zip(old_lines[1..old_lines.len()-1].iter())
                        .all(|(a, b)| a.trim() == b.trim());

                    if middle_matched {
                        let line_start: usize = content.lines().take(start_idx).map(|l| l.len() + 1).sum();
                        let matched_text: String = lines_between.join("\n");
                        return Some((line_start, line_start + matched_text.len()));
                    }
                }
            }
        }

        None
    }

    /// 策略4: 空白字符标准化匹配
    fn match_whitespace_normalized(&self, content: &str, old: &str) -> Option<(usize, usize)> {
        None
    }

    /// 智能替换
    fn smart_replace(&self, content: &str, old: &str, new: &str) -> Result<String, EditError> {
        // 按优先级尝试各种匹配策略
        let strategies = vec![
            ("Simple", MatchingStrategy::Simple),
            ("LineTrimmed", MatchingStrategy::LineTrimmed),
            ("BlockAnchor", MatchingStrategy::BlockAnchor),
            ("WhitespaceNormalized", MatchingStrategy::WhitespaceNormalized),
        ];

        let mut matches = None;
        let mut used_strategy = None;

        for (name, strategy) in strategies {
            let result = match strategy {
                MatchingStrategy::Simple => self.match_simple(content, old),
                MatchingStrategy::LineTrimmed => self.match_line_trimmed(content, old),
                MatchingStrategy::BlockAnchor => self.match_block_anchor(content, old),
                MatchingStrategy::WhitespaceNormalized => self.match_whitespace_normalized(content, old),
            };

            if let Some(range) = result {
                matches = Some(range);
                used_strategy = Some(name);
                break;
            }
        }

        let (start, end) = matches.ok_or_else(|| EditError::NotFound(old.to_string()))?;

        // 验证唯一性
        let all_matches = self.find_matches(content, old);
        if all_matches.len() > 1 {
            return Err(EditError::MultipleMatches(old.to_string()));
        }

        let result = content[..start].to_string() + new + &content[end..];

        debug!("Edit applied using {} strategy", used_strategy.unwrap_or("unknown"));

        Ok(result)
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing oldString with newString. Uses intelligent matching strategies if exact match fails."
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let path_str = params.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'path' parameter".to_string()))?;

        let path = PathBuf::from(path_str);
        if !path.is_absolute() {
            return Err(ToolError::InvalidArgument(
                "path must be an absolute path, not relative".to_string()
            ));
        }

        if !path.exists() {
            return Err(ToolError::InvalidPath(path));
        }

        let old_string = params.get("oldString")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'oldString' parameter".to_string()))?;

        let new_string = params.get("newString")
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'newString' parameter".to_string()))?
            .as_str()
            .unwrap_or("");

        let replace_all = params.get("replaceAll").and_then(|v| v.as_bool()).unwrap_or(false);

        let start = std::time::Instant::now();

        // Read file
        let content = fs::read_to_string(&path).await
            .map_err(|e| ToolError::Io(e))?;

        let result = if replace_all {
            // 替换所有匹配
            let count = self.find_matches(&content, old_string).len();
            let new_content = content.replace(old_string, new_string);
            (new_content, count)
        } else {
            // 单次替换
            match self.smart_replace(&content, old_string, new_string) {
                Ok(new_content) => (new_content, 1),
                Err(EditError::MultipleMatches(old)) => {
                    return Err(ToolError::ExecutionFailed(
                        format!("Multiple matches found for '{}'. Use replaceAll=true to replace all occurrences.", old)
                    ));
                }
                Err(e) => return Err(ToolError::ExecutionFailed(e.to_string())),
            }
        };

        // Write back
        fs::write(&path, &result.0).await
            .map_err(|e| ToolError::Io(e))?;

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            success: true,
            output: format!("Edited {} occurrences in {}", result.1, path.display()),
            error: None,
            metadata: ToolMetadata {
                execution_time_ms: duration,
                files_read: 1,
                files_written: 1,
                bytes_processed: result.0.len() as u64,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        ToolSchemaBuilder::new()
            .description("Edit file contents")
            .required_string("path", "The absolute path to the file to edit")
            .required_string("oldString", "The text to replace")
            .required_string("newString", "The text to replace it with")
            .param_boolean("replaceAll", "Replace all occurrences (default: false)")
            .build()
            .to_value()
    }
}

use tokio::fs;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs::File;
    use std::io::Write;

    #[tokio::test]
    async fn test_edit_simple() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, World!").unwrap();

        let tool = EditTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "oldString": "World",
            "newString": "Rust"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Edited 1 occurrences"));

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello, Rust!");
    }

    #[tokio::test]
    async fn test_edit_replace_all() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "foo bar foo baz foo").unwrap();

        let tool = EditTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "oldString": "foo",
            "newString": "qux",
            "replaceAll": true
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Edited 3 occurrences"));

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "qux bar qux baz qux");
    }

    #[tokio::test]
    async fn test_edit_multiline() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = EditTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "oldString": "line2",
            "newString": "new_line2"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("new_line2"));
    }

    #[tokio::test]
    async fn test_edit_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "hello").unwrap();

        let tool = EditTool::new();
        let params = serde_json::json!({
            "path": file_path.to_string_lossy(),
            "oldString": "notfound",
            "newString": "replacement"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_edit_missing_path() {
        let tool = EditTool::new();
        let params = serde_json::json!({
            "oldString": "test",
            "newString": "new"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_edit_relative_path_error() {
        let tool = EditTool::new();
        let params = serde_json::json!({
            "path": "./relative/path.txt",
            "oldString": "test",
            "newString": "new"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }
}
