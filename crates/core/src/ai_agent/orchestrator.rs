//! Agent Orchestrator - AI 交互中央控制器
//!
//! 职责:
//! - 协调 LLM Provider 与工具系统
//! - 管理对话历史和上下文
//! - 处理流式响应
//! - 实现反馈循环

use super::{AgentError, AgentToolCall, AgentToolResult, AgentSession, SessionState, TaskVerifier, VerificationResult, build_system_prompt, PromptContext};
use crate::llm::provider::{
    LlmProvider, CompletionRequest, Message, MessageRole, ToolCall, ToolCallFunction, ToolResult as LlmToolResult
};
use crate::{TaskId, AgentRole};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::Duration;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tracing::{info, warn, error};
use async_trait::async_trait;

/// Agent 配置
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// 最大工具调用次数 (防止无限循环)
    pub max_tool_calls: usize,

    /// 最大重试次数
    pub max_retries: usize,

    /// 是否启用流式响应
    pub enable_streaming: bool,

    /// 超时时间 (秒)
    pub timeout_secs: u64,

    /// 是否自动验证任务完成
    pub auto_verify: bool,

    /// 危险操作是否需要权限
    pub require_permission_for_dangerous: bool,

    /// 自定义系统提示词模板
    pub system_prompt_template: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tool_calls: 50,
            max_retries: 3,
            enable_streaming: true,
            timeout_secs: 300,
            auto_verify: true,
            require_permission_for_dangerous: true,
            system_prompt_template: None,
        }
    }
}

/// Agent 请求
#[derive(Debug, Clone)]
pub struct AgentRequest {
    /// 用户输入
    pub user_input: String,

    /// 会话 ID (可选，用于继续现有会话)
    pub session_id: Option<String>,

    /// 上下文文件路径
    pub working_dir: Option<std::path::PathBuf>,

    /// 当前角色
    pub role: Option<AgentRole>,

    /// 活跃任务 ID
    pub active_task_id: Option<TaskId>,
}

/// Agent 响应
#[derive(Debug, Clone)]
pub struct AgentResponse {
    /// 会话 ID
    pub session_id: String,

    /// AI 响应内容
    pub content: String,

    /// 使用的工具调用
    pub tool_calls: Vec<AgentToolCall>,

    /// 是否完成
    pub is_complete: bool,

    /// 是否需要用户输入
    pub needs_input: bool,

    /// 验证结果 (如果执行了验证)
    pub verification_result: Option<VerificationResult>,
}

/// 流式事件
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// 内容块
    Content(String),

    /// 工具调用
    ToolCall(AgentToolCall),

    /// 工具结果
    ToolResult(AgentToolResult),

    /// 完成
    Complete(AgentResponse),

    /// 错误
    Error(AgentError),
}

/// 工具执行器抽象
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// 执行工具调用
    async fn execute_tool(&self, name: &str, arguments: &str) -> Result<String, AgentError>;

    /// 获取可用工具列表
    fn list_tools(&self) -> Vec<String>;
}

/// Agent Orchestrator - 中央控制器
pub struct AgentOrchestrator {
    /// LLM Provider
    provider: Arc<dyn LlmProvider>,

    /// 工具执行器
    tool_executor: Arc<dyn ToolExecutor>,

    /// 任务验证器
    verifier: Arc<TaskVerifier>,

    /// 会话存储
    sessions: Arc<Mutex<HashMap<String, AgentSession>>>,

    /// 配置
    config: AgentConfig,
}

impl AgentOrchestrator {
    /// 创建新的 Agent Orchestrator
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        tool_executor: Arc<dyn ToolExecutor>,
        verifier: Arc<TaskVerifier>,
        config: AgentConfig,
    ) -> Self {
        Self {
            provider,
            tool_executor,
            verifier,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// 处理用户请求 (非流式)
    pub async fn process(&self, request: AgentRequest) -> Result<AgentResponse, AgentError> {
        info!("Processing agent request: {}", request.user_input);

        let start_time = std::time::Instant::now();
        let timeout = Duration::from_secs(self.config.timeout_secs);

        // 超时处理
        let process_fut = async {
            // 获取或创建会话
            let session_id = request.session_id.clone().unwrap_or_else(|| {
                ulid::Ulid::new().to_string()
            });

            let session = self.get_or_create_session(&session_id).await?;

            // 构建消息
            let user_message = Message {
                role: MessageRole::User,
                content: request.user_input.clone(),
                name: None,
                tool_calls: None,
            };

            // 执行主循环
            self.run_main_loop(session, user_message, request.active_task_id).await
        };

        tokio::select! {
            result = process_fut => result,
            _ = tokio::time::sleep(timeout) => {
                error!("Agent request timeout after {}s", self.config.timeout_secs);
                Err(AgentError::Timeout(self.config.timeout_secs))
            }
        }
    }

    /// 获取或创建会话
    async fn get_or_create_session(&self, session_id: &str) -> Result<AgentSession, AgentError> {
        let mut sessions = self.sessions.lock().await;

        if !sessions.contains_key(session_id) {
            let session = AgentSession::new(session_id.to_string());
            sessions.insert(session_id.to_string(), session);
            info!("Created new session: {}", session_id);
        }

        Ok(sessions.get(session_id).cloned().unwrap())
    }

    /// 主循环 - 非流式
    async fn run_main_loop(
        &self,
        session: AgentSession,
        user_message: Message,
        active_task_id: Option<TaskId>,
    ) -> Result<AgentResponse, AgentError> {
        let mut messages = self.build_messages(&session, &user_message, active_task_id).await?;

        let mut tool_call_count = 0;
        let mut all_tool_calls: Vec<AgentToolCall> = Vec::new();
        let mut final_content = String::new();

        loop {
            // 检查工具调用次数
            if tool_call_count >= self.config.max_tool_calls {
                warn!("Max tool calls exceeded: {}", tool_call_count);
                return Ok(AgentResponse {
                    session_id: session.id.clone(),
                    content: format!("I've reached the maximum number of tool calls ({}). Please review my progress and provide further guidance.", self.config.max_tool_calls),
                    tool_calls: all_tool_calls,
                    is_complete: false,
                    needs_input: true,
                    verification_result: None,
                });
            }

            // 调用 LLM
            let llm_request = CompletionRequest {
                model: self.provider.config().default_model.clone(),
                messages: messages.clone(),
                temperature: Some(0.1),
                max_tokens: Some(4096),
                top_p: None,
                frequency_penalty: None,
                presence_penalty: None,
                stop: None,
                stream: false,
            };

            let response = self.provider.complete(&llm_request)
                .await
                .map_err(|e| AgentError::LlmError(e.to_string()))?;

            // 获取助手响应
            let assistant_message = response.choices.first()
                .ok_or_else(|| AgentError::LlmError("No response from LLM".to_string()))?
                .message.clone();

            // 检查是否有工具调用
            if let Some(ref tool_calls) = assistant_message.tool_calls {
                if !tool_calls.is_empty() {
                    // 执行工具调用
                    let tool_results = self.execute_tool_calls(tool_calls).await?;

                    // 记录工具调用
                    for tc in tool_calls {
                        all_tool_calls.push(AgentToolCall {
                            name: tc.function.name.clone(),
                            arguments: tc.function.arguments.clone(),
                            id: tc.id.clone(),
                        });
                    }
                    tool_call_count += tool_calls.len();

                    // 添加助手消息和工具结果到历史
                    messages.push(assistant_message.clone());
                    for result in &tool_results {
                        messages.push(Message {
                            role: MessageRole::Tool,
                            content: result.content.clone(),
                            name: None,
                            tool_calls: None,
                        });
                    }

                    // 继续循环
                    continue;
                }
            }

            // 没有工具调用，获取最终内容
            final_content = assistant_message.content.clone();

            // 如果启用了自动验证且有活跃任务，执行验证
            let verification_result = if self.config.auto_verify {
                if let Some(task_id) = active_task_id {
                    self.verifier.verify_completion(&task_id).await.ok()
                } else {
                    None
                }
            } else {
                None
            };

            // 检查是否需要继续
            let needs_continuation = match verification_result {
                Some(VerificationResult::Incomplete { .. }) | Some(VerificationResult::QualityGateFailed { .. }) => true,
                _ => false,
            };

            if needs_continuation {
                // 添加反馈消息并继续
                let feedback = self.verifier.generate_continuation_prompt(
                    verification_result.as_ref().unwrap()
                );

                messages.push(Message {
                    role: MessageRole::System,
                    content: feedback,
                    name: None,
                    tool_calls: None,
                });

                // 继续循环
                continue;
            }

            // 完成
            break;
        }

        Ok(AgentResponse {
            session_id: session.id,
            content: final_content,
            tool_calls: all_tool_calls,
            is_complete: true,
            needs_input: false,
            verification_result: None,
        })
    }

    /// 构建消息列表
    async fn build_messages(
        &self,
        session: &AgentSession,
        user_message: &Message,
        _active_task_id: Option<TaskId>,
    ) -> Result<Vec<Message>, AgentError> {
        let mut messages = Vec::new();

        // 构建系统提示词
        let prompt_context = PromptContext {
            available_tools: vec![], // 工具列表由运行时提供
            active_task_id: _active_task_id,
            working_dir: None,
        };

        let system_prompt = if let Some(ref template) = self.config.system_prompt_template {
            template.clone()
        } else {
            build_system_prompt(&prompt_context)
        };

        messages.push(Message {
            role: MessageRole::System,
            content: system_prompt,
            name: None,
            tool_calls: None,
        });

        // 添加历史消息 (最近的 N 条)
        let history_limit = 20;
        for msg in session.messages.iter().rev().take(history_limit).rev() {
            messages.push(Message {
                role: msg.role.clone(),
                content: msg.content.clone(),
                name: None,
                tool_calls: None,
            });
        }

        // 添加当前用户消息
        messages.push(user_message.clone());

        Ok(messages)
    }

    /// 执行工具调用
    async fn execute_tool_calls(
        &self,
        tool_calls: &[ToolCall],
    ) -> Result<Vec<LlmToolResult>, AgentError> {
        let mut results = Vec::new();

        for tool_call in tool_calls {
            let function = &tool_call.function;
            let tool_name = &function.name;

            info!("Executing tool: {}", tool_name);

            // 执行工具
            let tool_result = match self.tool_executor.execute_tool(tool_name, &function.arguments).await {
                Ok(content) => LlmToolResult {
                    tool_call_id: tool_call.id.clone(),
                    content,
                    is_error: false,
                },
                Err(e) => {
                    warn!("Tool {} execution failed: {}", tool_name, e);
                    LlmToolResult {
                        tool_call_id: tool_call.id.clone(),
                        content: format!("Error: {}", e),
                        is_error: true,
                    }
                }
            };

            results.push(tool_result);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert_eq!(config.max_tool_calls, 50);
        assert_eq!(config.max_retries, 3);
        assert!(config.enable_streaming);
        assert_eq!(config.timeout_secs, 300);
        assert!(config.auto_verify);
        assert!(config.require_permission_for_dangerous);
    }

    #[test]
    fn test_agent_request() {
        let request = AgentRequest {
            user_input: "Create a new task".to_string(),
            session_id: None,
            working_dir: None,
            role: None,
            active_task_id: None,
        };

        assert_eq!(request.user_input, "Create a new task");
        assert!(request.session_id.is_none());
    }

    #[test]
    fn test_agent_response() {
        let response = AgentResponse {
            session_id: "test-session".to_string(),
            content: "Task created successfully".to_string(),
            tool_calls: vec![],
            is_complete: true,
            needs_input: false,
            verification_result: None,
        };

        assert_eq!(response.session_id, "test-session");
        assert!(response.is_complete);
        assert!(!response.needs_input);
    }

    #[test]
    fn test_stream_event() {
        let event = StreamEvent::Content("Hello".to_string());
        match event {
            StreamEvent::Content(s) => assert_eq!(s, "Hello"),
            _ => panic!("Expected Content event"),
        }
    }

    #[test]
    fn test_agent_tool_call() {
        let call = AgentToolCall {
            name: "test_tool".to_string(),
            arguments: r#"{"param": "value"}"#.to_string(),
            id: "call-123".to_string(),
        };

        assert_eq!(call.name, "test_tool");
        assert_eq!(call.id, "call-123");
    }

    #[test]
    fn test_agent_tool_result() {
        let result = AgentToolResult {
            tool_call_id: "call-123".to_string(),
            content: "Success".to_string(),
            is_error: false,
            metadata: Default::default(),
        };

        assert_eq!(result.tool_call_id, "call-123");
        assert!(!result.is_error);
    }
}
