//! Tool Trait - 工具接口定义
//!
//! 设计原则：
//! - 所有工具实现统一的 Tool Trait
//! - 参数和结果使用 JSON 序列化
//! - 统一的错误处理
//! - 支持审计日志

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

/// 工具执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub metadata: ToolMetadata,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub execution_time_ms: u64,
    pub files_read: u32,
    pub files_written: u32,
    pub bytes_processed: u64,
}

/// 工具错误
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("序列化错误: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("工具不存在: {0}")]
    NotFound(String),

    #[error("执行失败: {0}")]
    ExecutionFailed(String),

    #[error("权限拒绝: {0}")]
    PermissionDenied(String),

    #[error("路径无效: {0}")]
    InvalidPath(PathBuf),

    #[error("参数错误: {0}")]
    InvalidArgument(String),

    #[error("执行超时: {0}")]
    Timeout(String),
}

/// 工具参数（JSON 序列化）
pub type ToolParams = serde_json::Value;

/// Tool Trait - 所有工具必须实现
#[async_trait]
pub trait Tool: Send + Sync {
    /// 工具名称
    fn name(&self) -> &str;

    /// 工具描述
    fn description(&self) -> &str;

    /// 执行工具
    async fn execute(&self, params: &ToolParams) -> Result<ToolResult, ToolError>;

    /// 获取参数模式（用于验证）
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }
}

/// 工具执行上下文
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// 当前工作目录
    pub working_dir: PathBuf,

    /// 环境变量
    pub env_vars: std::collections::HashMap<String, String>,

    /// 允许的操作列表
    pub allowed_operations: Vec<String>,

    /// 是否为只读模式
    pub read_only: bool,

    /// 超时时间（秒）
    pub timeout_seconds: u64,
}

impl Default for ToolContext {
    fn default() -> Self {
        Self {
            working_dir: std::env::current_dir().unwrap_or(PathBuf::from(".")),
            env_vars: std::env::vars().collect(),
            allowed_operations: vec!["read".to_string(), "write".to_string()],
            read_only: false,
            timeout_seconds: 300,
        }
    }
}

/// 工具管理器
#[derive(Default)]
pub struct ToolManager {
    registry: std::collections::HashMap<String, Arc<dyn Tool>>,
    #[allow(dead_code)]
    context: ToolContext,
}

impl std::fmt::Debug for ToolManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolManager")
            .field("tool_names", &self.registry.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ToolManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T: Tool + 'static>(&mut self, name: impl Into<String>, tool: T) {
        self.registry.insert(name.into(), Arc::new(tool));
    }

    pub async fn execute(
        &self,
        tool_name: &str,
        params: &ToolParams,
    ) -> Result<ToolResult, ToolError> {
        let tool = self
            .registry
            .get(tool_name)
            .ok_or_else(|| ToolError::NotFound(tool_name.to_string()))?;

        let start = std::time::Instant::now();
        let result = tool.execute(params).await?;
        let duration = start.elapsed().as_millis() as u64;

        tracing::debug!(
            tool = tool_name,
            duration_ms = duration,
            success = result.success
        );

        Ok(result)
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.registry.get(name)
    }
}
