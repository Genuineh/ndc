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

pub mod adapters;
pub mod injectors;
pub mod orchestrator;
pub mod prompts;
pub mod session;
pub mod verifier;

pub use orchestrator::{
    AgentConfig, AgentOrchestrator, AgentRequest, AgentResponse, StreamEvent, ToolExecutor,
};

pub use session::{AgentMessage, AgentSession, SessionManager, SessionState};

pub use verifier::{TaskStorage, TaskVerifier, VerificationError, VerificationResult};

pub use prompts::{PromptBuilder, PromptContext, build_system_prompt};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

use crate::llm::provider::ProviderError;

impl From<ProviderError> for AgentError {
    fn from(err: ProviderError) -> Self {
        AgentError::LlmError(err.to_string())
    }
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

/// 执行事件类型（用于多轮可视化与时间线）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentExecutionEventKind {
    WorkflowStage,
    StepStart,
    StepFinish,
    ToolCallStart,
    ToolCallEnd,
    TokenUsage,
    Reasoning,
    Text,
    Verification,
    PermissionAsked,
    SessionStatus,
    Error,
}

/// Workflow 阶段（统一语义）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentWorkflowStage {
    Planning,
    Discovery,
    Executing,
    Verifying,
    Completing,
}

impl AgentWorkflowStage {
    pub const TOTAL_STAGES: u32 = 5;

    pub fn as_str(self) -> &'static str {
        match self {
            AgentWorkflowStage::Planning => "planning",
            AgentWorkflowStage::Discovery => "discovery",
            AgentWorkflowStage::Executing => "executing",
            AgentWorkflowStage::Verifying => "verifying",
            AgentWorkflowStage::Completing => "completing",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "planning" => Some(AgentWorkflowStage::Planning),
            "discovery" => Some(AgentWorkflowStage::Discovery),
            "executing" => Some(AgentWorkflowStage::Executing),
            "verifying" => Some(AgentWorkflowStage::Verifying),
            "completing" => Some(AgentWorkflowStage::Completing),
            _ => None,
        }
    }

    pub fn index(self) -> u32 {
        match self {
            AgentWorkflowStage::Planning => 1,
            AgentWorkflowStage::Discovery => 2,
            AgentWorkflowStage::Executing => 3,
            AgentWorkflowStage::Verifying => 4,
            AgentWorkflowStage::Completing => 5,
        }
    }
}

impl std::fmt::Display for AgentWorkflowStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 单条执行事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionEvent {
    /// 事件类型
    pub kind: AgentExecutionEventKind,
    /// 事件时间
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 简要描述
    pub message: String,
    /// 会话内轮次（从 1 开始）
    pub round: usize,
    /// 工具名（如果有）
    pub tool_name: Option<String>,
    /// 工具调用 ID（如果有）
    pub tool_call_id: Option<String>,
    /// 耗时（毫秒）
    pub duration_ms: Option<u64>,
    /// 是否错误
    pub is_error: bool,
    /// 结构化 workflow stage（仅 WorkflowStage 事件）
    #[serde(default)]
    pub workflow_stage: Option<AgentWorkflowStage>,
    /// 结构化 workflow detail（仅 WorkflowStage 事件）
    #[serde(default)]
    pub workflow_detail: Option<String>,
    /// workflow stage index（从 1 开始）
    #[serde(default)]
    pub workflow_stage_index: Option<u32>,
    /// workflow stage total（通常为常量 TOTAL_STAGES）
    #[serde(default)]
    pub workflow_stage_total: Option<u32>,
}

/// Workflow 阶段解析结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentWorkflowStageInfo {
    pub stage: AgentWorkflowStage,
    pub detail: String,
    pub index: u32,
    pub total: u32,
}

/// Token 使用量解析结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTokenUsageInfo {
    pub source: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub session_prompt_total: u64,
    pub session_completion_total: u64,
    pub session_total: u64,
}

fn parse_metric_value<'a>(message: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{}=", key);
    message
        .split(|ch: char| ch.is_whitespace() || ch == '|')
        .find_map(|token| token.trim().strip_prefix(prefix.as_str()))
}

impl AgentExecutionEvent {
    /// Parse `WorkflowStage` payload from the canonical event message.
    pub fn workflow_stage_info(&self) -> Option<AgentWorkflowStageInfo> {
        if self.kind != AgentExecutionEventKind::WorkflowStage {
            return None;
        }
        if let Some(stage) = self.workflow_stage {
            let detail = self.workflow_detail.clone().unwrap_or_default();
            return Some(AgentWorkflowStageInfo {
                stage,
                detail,
                index: self.workflow_stage_index.unwrap_or_else(|| stage.index()),
                total: self
                    .workflow_stage_total
                    .unwrap_or(AgentWorkflowStage::TOTAL_STAGES),
            });
        }

        let rest = self.message.strip_prefix("workflow_stage:")?.trim();
        let mut parts = rest.splitn(2, '|');
        let stage = AgentWorkflowStage::parse(parts.next()?.trim())?;
        let detail = parts.next().map(|s| s.trim()).unwrap_or_default();
        Some(AgentWorkflowStageInfo {
            stage,
            detail: detail.to_string(),
            index: stage.index(),
            total: AgentWorkflowStage::TOTAL_STAGES,
        })
    }

    /// Parse `TokenUsage` payload from the canonical event message.
    pub fn token_usage_info(&self) -> Option<AgentTokenUsageInfo> {
        if self.kind != AgentExecutionEventKind::TokenUsage {
            return None;
        }
        let parse_u64 = |key: &str| -> u64 {
            parse_metric_value(&self.message, key)
                .and_then(|value| value.trim_end_matches(',').parse::<u64>().ok())
                .unwrap_or(0)
        };
        Some(AgentTokenUsageInfo {
            source: parse_metric_value(&self.message, "source")
                .unwrap_or("unknown")
                .to_string(),
            prompt_tokens: parse_u64("prompt"),
            completion_tokens: parse_u64("completion"),
            total_tokens: parse_u64("total"),
            session_prompt_total: parse_u64("session_prompt_total"),
            session_completion_total: parse_u64("session_completion_total"),
            session_total: parse_u64("session_total"),
        })
    }
}

/// 带会话标识的执行事件（用于实时订阅）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSessionExecutionEvent {
    pub session_id: String,
    pub event: AgentExecutionEvent,
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
    fn test_workflow_stage_info_parsing() {
        let event = AgentExecutionEvent {
            kind: AgentExecutionEventKind::WorkflowStage,
            timestamp: chrono::Utc::now(),
            message: "workflow_stage: executing | llm_round_start".to_string(),
            round: 1,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: Some(AgentWorkflowStage::Executing),
            workflow_detail: Some("llm_round_start".to_string()),
            workflow_stage_index: Some(3),
            workflow_stage_total: Some(AgentWorkflowStage::TOTAL_STAGES),
        };
        let info = event.workflow_stage_info().expect("workflow info");
        assert_eq!(info.stage, AgentWorkflowStage::Executing);
        assert_eq!(info.detail, "llm_round_start");
        assert_eq!(info.index, 3);
        assert_eq!(info.total, AgentWorkflowStage::TOTAL_STAGES);
    }

    #[test]
    fn test_token_usage_info_parsing() {
        let event = AgentExecutionEvent {
            kind: AgentExecutionEventKind::TokenUsage,
            timestamp: chrono::Utc::now(),
            message: "token_usage: source=provider prompt=11 completion=7 total=18 | session_prompt_total=22 session_completion_total=14 session_total=36".to_string(),
            round: 2,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        };
        let info = event.token_usage_info().expect("token info");
        assert_eq!(info.source, "provider");
        assert_eq!(info.prompt_tokens, 11);
        assert_eq!(info.completion_tokens, 7);
        assert_eq!(info.total_tokens, 18);
        assert_eq!(info.session_prompt_total, 22);
        assert_eq!(info.session_completion_total, 14);
        assert_eq!(info.session_total, 36);
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
