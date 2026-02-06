//! FsTool - 文件系统操作工具
//!
//! 提供安全的文件读写操作：
//! - 读取文件内容
//! - 写入文件
//! - 创建文件/目录
//! - 删除文件
//! - 列出目录内容

use super::{Tool, ToolResult, ToolError, ToolContext};
use tokio::fs;
use std::path::{PathBuf, Path};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// 文件系统工具
#[derive(Debug)]
pub struct FsTool {
    /// 执行上下文
    context: ToolContext,
}

impl FsTool {
    pub fn new() -> Self {
        Self {
            context: ToolContext::default(),
        }
    }

    /// 验证路径安全性
    fn validate_path(&self, path: &PathBuf) -> Result<PathBuf, ToolError> {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            self.context.working_dir.join(path)
        };

        // 规范化路径
        let canonical = fs::canonicalize(&absolute)
            .await
            .map_err(|_| ToolError::InvalidPath(path.clone()))?;

        // 确保路径在工作目录内（防止路径遍历攻击）
        let work_dir = fs::canonicalize(&self.context.working_dir).await?;

        if !canonical.starts_with(&work_dir) {
            warn!("Path traversal attempt: {:?}", path);
            return Err(ToolError::InvalidPath(path.clone()));
        }

        Ok(canonical)
    }
}

impl Default for FsTool {
    fn default() -> Self {
        Self::new()
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
        let operation = params.get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'operation'".to_string()))?;

        let path = params.get("path")
            .and_then(|v| v.as_str())
            .map(|s| PathBuf::from(s))
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'path'".to_string()))?;

        // 验证路径
        let validated_path = self.validate_path(&path)?;

        let start = std::time::Instant::now();
        let mut output = String::new();
        let mut files_read = 0;
        let mut files_written = 0;
        let mut bytes_processed = 0u64;

        match operation {
            "read" => {
                // 检查是否允许读取
                if !self.context.allowed_operations.contains(&"read".to_string()) {
                    return Err(ToolError::PermissionDenied("Read not allowed".to_string()));
                }

                // 读取文件
                let content = fs::read_to_string(&validated_path).await
                    .map_err(|e| ToolError::Io(e))?;

                bytes_processed = content.len() as u64;
                files_read = 1;
                output = format!("Read {} bytes from {}", bytes_processed, validated_path.display());
                debug!("Read file: {:?}", validated_path);
            }

            "write" => {
                // 检查是否为只读模式
                if self.context.read_only {
                    return Err(ToolError::PermissionDenied("Write not allowed in read-only mode".to_string()));
                }

                // 检查是否允许写入
                if !self.context.allowed_operations.contains(&"write".to_string()) {
                    return Err(ToolError::PermissionDenied("Write not allowed".to_string()));
                }

                let content = params.get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArgument("Missing 'content' for write".to_string()))?;

                // 确保父目录存在
                if let Some(parent) = validated_path.parent() {
                    fs::create_dir_all(parent).await
                        .map_err(|e| ToolError::Io(e))?;
                }

                // 写入文件
                fs::write(&validated_path, content).await
                    .map_err(|e| ToolError::Io(e))?;

                bytes_processed = content.len() as u64;
                files_written = 1;
                output = format!("Wrote {} bytes to {}", bytes_processed, validated_path.display());
                debug!("Wrote file: {:?}", validated_path);
            }

            "create" => {
                if self.context.read_only {
                    return Err(ToolError::PermissionDenied("Create not allowed in read-only mode".to_string()));
                }

                // 检查是否是目录
                if params.get("is_directory").and_then(|v| v.as_bool()).unwrap_or(false) {
                    fs::create_dir_all(&validated_path).await
                        .map_err(|e| ToolError::Io(e))?;
                    output = format!("Created directory: {}", validated_path.display());
                } else {
                    // 创建空文件
                    fs::write(&validated_path, "").await
                        .map_err(|e| ToolError::Io(e))?;
                    output = format!("Created file: {}", validated_path.display());
                }
                files_written = 1;
                debug!("Created: {:?}", validated_path);
            }

            "delete" => {
                if self.context.read_only {
                    return Err(ToolError::PermissionDenied("Delete not allowed in read-only mode".to_string()));
                }

                // 检查是否允许删除
                if !self.context.allowed_operations.contains(&"delete".to_string()) {
                    return Err(ToolError::PermissionDenied("Delete not allowed".to_string()));
                }

                // 先检查是否是目录
                let metadata = fs::metadata(&validated_path).await
                    .map_err(|e| ToolError::Io(e))?;

                if metadata.is_dir() {
                    fs::remove_dir_all(&validated_path).await
                        .map_err(|e| ToolError::Io(e))?;
                    output = format!("Deleted directory: {}", validated_path.display());
                } else {
                    fs::remove_file(&validated_path).await
                        .map_err(|e| ToolError::Io(e))?;
                    output = format!("Deleted file: {}", validated_path.display());
                }
                files_written = 1;
                debug!("Deleted: {:?}", validated_path);
            }

            "list" => {
                if !self.context.allowed_operations.contains(&"read".to_string()) {
                    return Err(ToolError::PermissionDenied("List not allowed".to_string()));
                }

                let mut entries = fs::read_dir(&validated_path).await
                    .map_err(|e| ToolError::Io(e))?;

                let mut items = Vec::new();
                while let Some(entry) = entries.next_entry().await
                    .map_err(|e| ToolError::Io(e))? {
                    items.push(entry.file_name().to_string_lossy().to_string());
                }

                output = serde_json::to_string(&items)
                    .map_err(|e| ToolError::Serialize(e))?;
                files_read = 1;
                debug!("Listed directory: {:?}", validated_path);
            }

            "exists" => {
                let exists = validated_path.exists();
                output = format!("{}", exists);
                debug!("Checked existence: {:?}", validated_path);
            }

            "metadata" => {
                let metadata = fs::metadata(&validated_path).await
                    .map_err(|e| ToolError::Io(e))?;

                let meta = FileMetadata {
                    is_file: metadata.is_file(),
                    is_directory: metadata.is_dir(),
                    size: metadata.len(),
                    modified: metadata.modified()
                        .ok()
                        .map(|v| v.elapsed().ok())
                        .flatten()
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                };

                output = serde_json::to_string(&meta)
                    .map_err(|e| ToolError::Serialize(e))?;
                files_read = 1;
                debug!("Got metadata: {:?}", validated_path);
            }

            _ => {
                return Err(ToolError::InvalidArgument(
                    format!("Unknown operation: {}", operation)
                ));
            }
        }

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            success: true,
            output,
            error: None,
            metadata: ToolMetadata {
                execution_time_ms: duration,
                files_read,
                files_written,
                bytes_processed,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        serde::json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["read", "write", "create", "delete", "list", "exists", "metadata"],
                    "description": "File operation type"
                },
                "path": {
                    "type": "string",
                    "description": "File path"
                },
                "content": {
                    "type": "string",
                    "description": "File content for write operation"
                },
                "is_directory": {
                    "type": "boolean",
                    "description": "Create as directory"
                }
            },
            "required": ["operation", "path"]
        })
    }
}

/// 文件元数据
#[derive(Debug, Serialize, Deserialize)]
pub struct FileMetadata {
    pub is_file: bool,
    pub is_directory: bool,
    pub size: u64,
    pub modified: u64,
}
