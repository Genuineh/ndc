//! NDC AI Agent Module
//!
//! 职责:
//! - Agent Orchestrator: AI 交互中央控制器
//! - Session Management: 会话状态管理
//! - NDC Tools: 将内部系统功能暴露为 AI 工具
//! - Verification: 任务完成验证与反馈循环
//!
//! 设计理念:
//! - 工具化内部流程 - 内部系统功能变成 AI 可调用的工具
//! - 反馈驱动 - AI 完成后系统验证，未完成则要求继续
//! - 流式响应 - 实时展示 AI 思考过程
//! - 权限控制 - 危险操作需要人工确认

pub mod orchestrator;
pub mod session;
pub mod verifier;
pub mod prompts;

pub use orchestrator::{
    AgentOrchestrator,
    AgentConfig,
    AgentRequest,
    AgentResponse,
    StreamEvent,
    ToolExecutor,
};

pub use session::{
    AgentSession,
    SessionState,
    SessionManager,
    AgentMessage,
};

pub use verifier::{
    TaskVerifier,
    VerificationResult,
    VerificationError,
    TaskStorage,
};

pub use prompts::{
    build_system_prompt,
    PromptBuilder,
    PromptContext,
};

use crate::llm::provider::{LlmProvider, CompletionRequest, Message, MessageRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;
use thiserror::Error;

/// Agent 错误类型
#[derive(Debug, Error, Clone)]
pub enum AgentError {
    #[error("LLM provider error: {0}")]
    LlmError(String),

    #[error("Tool execution error: {0}")]
    ToolError(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Timeout: operation exceeded {0}s")]
    Timeout(u64),

    #[error("Max tool calls exceeded: {0}")]
    MaxToolCallsExceeded(usize),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Other error: {0}")]
    Other(String),
}

/// 工具调用包装器 - 用于 AI 工具系统
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolCall {
    /// 工具名称
    pub name: String,

    /// 工具参数 (JSON 字符串)
    pub arguments: String,

    /// 调用 ID
    pub id: String,
}

/// 工具结果包装器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolResult {
    /// 对应的工具调用 ID
    pub tool_call_id: String,

    /// 结果内容
    pub content: String,

    /// 是否成功
    pub is_error: bool,

    /// 元数据
    pub metadata: HashMap<String, String>,
}

/// Agent 元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    /// Agent 版本
    pub version: String,

    /// 支持的工具列表
    pub available_tools: Vec<String>,

    /// 当前会话数
    pub active_sessions: usize,

    /// 总工具调用次数
    pub total_tool_calls: u64,
}

impl Default for AgentMetadata {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            available_tools: Vec::new(),
            active_sessions: 0,
            total_tool_calls: 0,
        }
    }
}

/// 创建默认的 Agent 配置
pub fn default_agent_config() -> AgentConfig {
    AgentConfig {
        max_tool_calls: 50,
        max_retries: 3,
        enable_streaming: true,
        timeout_secs: 300,
        auto_verify: true,
        require_permission_for_dangerous: true,
        system_prompt_template: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_metadata_default() {
        let meta = AgentMetadata::default();
        assert!(!meta.version.is_empty());
        assert_eq!(meta.active_sessions, 0);
        assert_eq!(meta.total_tool_calls, 0);
    }

    #[test]
    fn test_default_agent_config() {
        let config = default_agent_config();
        assert_eq!(config.max_tool_calls, 50);
        assert_eq!(config.max_retries, 3);
        assert!(config.enable_streaming);
        assert_eq!(config.timeout_secs, 300);
        assert!(config.auto_verify);
        assert!(config.require_permission_for_dangerous);
    }

    #[test]
    fn test_agent_tool_call_serialization() {
        let call = AgentToolCall {
            name: "test_tool".to_string(),
            arguments: r#"{"param": "value"}"#.to_string(),
            id: "call-123".to_string(),
        };

        let json = serde_json::to_string(&call).unwrap();
        let parsed: AgentToolCall = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "test_tool");
        // 比较 JSON 解析后的值而非字符串格式
        let orig: serde_json::Value = serde_json::from_str(&call.arguments).unwrap();
        let parsed_args: serde_json::Value = serde_json::from_str(&parsed.arguments).unwrap();
        assert_eq!(orig, parsed_args);
        assert_eq!(parsed.id, "call-123");
    }

    #[test]
    fn test_agent_tool_result_serialization() {
        let result = AgentToolResult {
            tool_call_id: "call-123".to_string(),
            content: "Success".to_string(),
            is_error: false,
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: AgentToolResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.tool_call_id, "call-123");
        assert_eq!(parsed.content, "Success");
        assert!(!parsed.is_error);
    }

    #[test]
    fn test_agent_error_display() {
        let err = AgentError::PermissionDenied("Operation not allowed".to_string());
        assert!(err.to_string().contains("Permission denied"));
        assert!(err.to_string().contains("Operation not allowed"));
    }
}
