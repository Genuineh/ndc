//! Agent Orchestrator - AI 交互中央控制器
//!
//! 职责:
//! - 协调 LLM Provider 与工具系统
//! - 管理对话历史和上下文
//! - 处理流式响应
//! - 实现反馈循环

use super::{
    AgentError, AgentExecutionEvent, AgentExecutionEventKind, AgentMessage, AgentSession,
    AgentSessionExecutionEvent, AgentToolCall, AgentToolResult, AgentWorkflowStage, TaskVerifier,
    VerificationResult,
    injectors::working_memory::{TaskContext, WorkingMemoryContext, WorkingMemoryInjector},
    prompts::{EnhancedPromptContext, build_enhanced_prompt},
};
use crate::llm::provider::{
    CompletionRequest, LlmProvider, Message, MessageRole, ProviderError, StreamHandler, ToolCall,
    ToolResult as LlmToolResult,
};
use crate::{AgentRole, TaskId};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, broadcast};
use tracing::{error, info, warn};

/// 流式响应处理器 - 使用 Mutex 包装内容
struct StreamingHandler {
    content: Arc<Mutex<String>>,
}

impl StreamingHandler {
    fn new(content: Arc<Mutex<String>>) -> Self {
        Self { content }
    }
}

#[async_trait::async_trait]
impl StreamHandler for StreamingHandler {
    async fn on_chunk(
        &self,
        chunk: &crate::llm::provider::StreamChunk,
    ) -> Result<(), ProviderError> {
        let mut content = self.content.lock().await;
        for choice in &chunk.choices {
            if let Some(delta) = &choice.delta {
                if !delta.content.is_empty() {
                    content.push_str(&delta.content);
                }
            }
        }
        Ok(())
    }

    async fn on_complete(
        &self,
        _response: &crate::llm::provider::CompletionResponse,
    ) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn on_error(&self, error: &ProviderError) {
        error!("Streaming error: {:?}", error);
    }
}

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

    /// Optional working memory (Abstract + Raw + Hard)
    pub working_memory: Option<crate::WorkingMemory>,
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

    /// 执行事件（用于可视化时间线）
    pub execution_events: Vec<AgentExecutionEvent>,
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

    /// 获取可供 LLM 使用的工具 schema（OpenAI function calling format）
    fn tool_schemas(&self) -> Vec<serde_json::Value> {
        Vec::new()
    }
}

/// Agent Orchestrator - 中央控制器
#[derive(Clone)]
pub struct AgentOrchestrator {
    /// LLM Provider
    provider: Arc<dyn LlmProvider>,

    /// 工具执行器
    tool_executor: Arc<dyn ToolExecutor>,

    /// 任务验证器
    verifier: Arc<TaskVerifier>,

    /// 会话存储
    sessions: Arc<Mutex<HashMap<String, AgentSession>>>,

    /// 实时执行事件总线
    event_tx: broadcast::Sender<AgentSessionExecutionEvent>,

    /// 配置
    config: AgentConfig,
}

impl AgentOrchestrator {
    async fn emit_event(
        &self,
        session_state: &mut AgentSession,
        execution_events: &mut Vec<AgentExecutionEvent>,
        event: AgentExecutionEvent,
    ) {
        let _ = self.event_tx.send(AgentSessionExecutionEvent {
            session_id: session_state.id.clone(),
            event: event.clone(),
        });
        session_state.add_execution_event(event.clone());
        execution_events.push(event);
        self.save_session(session_state.clone()).await;
    }

    async fn emit_workflow_stage(
        &self,
        session_state: &mut AgentSession,
        execution_events: &mut Vec<AgentExecutionEvent>,
        round: usize,
        stage: AgentWorkflowStage,
        detail: &str,
    ) {
        self.emit_event(
            session_state,
            execution_events,
            AgentExecutionEvent {
                kind: AgentExecutionEventKind::WorkflowStage,
                timestamp: chrono::Utc::now(),
                message: format!("workflow_stage: {} | {}", stage.as_str(), detail),
                round,
                tool_name: None,
                tool_call_id: None,
                duration_ms: None,
                is_error: false,
                workflow_stage: Some(stage),
                workflow_detail: Some(detail.to_string()),
                workflow_stage_index: Some(stage.index()),
                workflow_stage_total: Some(AgentWorkflowStage::TOTAL_STAGES),
            },
        )
        .await;
    }

    async fn emit_token_usage(
        &self,
        session_state: &mut AgentSession,
        execution_events: &mut Vec<AgentExecutionEvent>,
        round: usize,
        usage: crate::llm::provider::Usage,
        session_prompt_total: u64,
        session_completion_total: u64,
        session_total: u64,
        estimated: bool,
    ) {
        let source = if estimated { "estimated" } else { "provider" };
        self.emit_event(
            session_state,
            execution_events,
            AgentExecutionEvent {
                kind: AgentExecutionEventKind::TokenUsage,
                timestamp: chrono::Utc::now(),
                message: format!(
                    "token_usage: source={} prompt={} completion={} total={} | session_prompt_total={} session_completion_total={} session_total={}",
                    source,
                    usage.prompt_tokens,
                    usage.completion_tokens,
                    usage.total_tokens,
                    session_prompt_total,
                    session_completion_total,
                    session_total
                ),
                round,
                tool_name: None,
                tool_call_id: None,
                duration_ms: None,
                is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
            },
        )
        .await;
    }

    /// 创建新的 Agent Orchestrator
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        tool_executor: Arc<dyn ToolExecutor>,
        verifier: Arc<TaskVerifier>,
        config: AgentConfig,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(2048);
        Self {
            provider,
            tool_executor,
            verifier,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
            config,
        }
    }

    /// 订阅实时执行事件
    pub fn subscribe_execution_events(&self) -> broadcast::Receiver<AgentSessionExecutionEvent> {
        self.event_tx.subscribe()
    }

    /// 处理用户请求 (非流式)
    pub async fn process(&self, request: AgentRequest) -> Result<AgentResponse, AgentError> {
        info!("Processing agent request: {}", request.user_input);

        let timeout = Duration::from_secs(self.config.timeout_secs);

        // 超时处理
        let process_fut = async {
            // 获取或创建会话
            let session_id = request
                .session_id
                .clone()
                .unwrap_or_else(|| ulid::Ulid::new().to_string());

            let session = self.get_or_create_session(&session_id).await?;

            // 构建消息
            let user_message = Message {
                role: MessageRole::User,
                content: request.user_input.clone(),
                name: None,
                tool_calls: None,
            };

            // 执行主循环
            self.run_main_loop(
                session,
                user_message,
                request.active_task_id,
                request.working_dir.clone(),
                request.working_memory.clone(),
            )
            .await
        };

        tokio::select! {
            result = process_fut => result,
            _ = tokio::time::sleep(timeout) => {
                error!("Agent request timeout after {}s", self.config.timeout_secs);
                Err(AgentError::Timeout(self.config.timeout_secs))
            }
        }
    }

    /// 处理用户请求 (流式)
    pub async fn process_streaming<F>(
        &self,
        request: AgentRequest,
        _on_chunk: F,
    ) -> Result<AgentResponse, AgentError>
    where
        F: FnMut(String) + Send + 'static,
    {
        info!("Processing streaming agent request: {}", request.user_input);

        // 获取或创建会话
        let session_id = request
            .session_id
            .clone()
            .unwrap_or_else(|| ulid::Ulid::new().to_string());

        let session = self.get_or_create_session(&session_id).await?;

        // 构建消息
        let user_message = Message {
            role: MessageRole::User,
            content: request.user_input.clone(),
            name: None,
            tool_calls: None,
        };

        let messages = self
            .build_messages(
                &session,
                &user_message,
                request.active_task_id,
                request.working_dir.clone(),
                request.working_memory.clone(),
            )
            .await?;

        // 构建流式请求
        let tool_schemas = self.tool_executor.tool_schemas();
        let llm_request = CompletionRequest {
            model: self.provider.config().default_model.clone(),
            messages,
            temperature: Some(0.1),
            max_tokens: Some(4096),
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            stream: true,
            tools: if tool_schemas.is_empty() {
                None
            } else {
                Some(tool_schemas)
            },
        };

        // 创建流处理器
        let content = Arc::new(Mutex::new(String::new()));
        let handler: Arc<dyn StreamHandler> = Arc::new(StreamingHandler::new(content.clone()));

        // 发送流式请求
        self.provider
            .complete_streaming(&llm_request, &handler)
            .await?;

        // 获取累积的内容
        let final_content = {
            let c = content.lock().await;
            c.clone()
        };

        let mut session_state = session.clone();
        session_state.add_message(AgentMessage {
            role: MessageRole::User,
            content: user_message.content.clone(),
            timestamp: chrono::Utc::now(),
            tool_calls: None,
            tool_results: None,
        });
        session_state.add_message(AgentMessage {
            role: MessageRole::Assistant,
            content: final_content.clone(),
            timestamp: chrono::Utc::now(),
            tool_calls: None,
            tool_results: None,
        });
        self.save_session(session_state).await;

        Ok(AgentResponse {
            session_id,
            content: final_content,
            tool_calls: Vec::new(),
            is_complete: true,
            needs_input: false,
            verification_result: None,
            execution_events: Vec::new(),
        })
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

    async fn save_session(&self, session: AgentSession) {
        let mut sessions = self.sessions.lock().await;
        sessions.insert(session.id.clone(), session);
    }

    /// 获取会话执行事件时间线（用于回放/可视化）
    pub async fn get_session_execution_events(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AgentExecutionEvent>, AgentError> {
        let sessions = self.sessions.lock().await;
        let session = sessions.get(session_id).ok_or_else(|| {
            AgentError::SessionNotFound(format!("Session '{}' not found", session_id))
        })?;
        let events = &session.execution_events;
        let max = limit.unwrap_or(events.len());
        let start = events.len().saturating_sub(max);
        Ok(events[start..].to_vec())
    }

    /// 主循环 - 非流式
    async fn run_main_loop(
        &self,
        session: AgentSession,
        user_message: Message,
        active_task_id: Option<TaskId>,
        working_dir: Option<std::path::PathBuf>,
        working_memory: Option<crate::WorkingMemory>,
    ) -> Result<AgentResponse, AgentError> {
        let mut messages = self
            .build_messages(
                &session,
                &user_message,
                active_task_id.clone(),
                working_dir.clone(),
                working_memory,
            )
            .await?;
        let mut session_state = session.clone();
        session_state.add_message(AgentMessage {
            role: MessageRole::User,
            content: user_message.content.clone(),
            timestamp: chrono::Utc::now(),
            tool_calls: None,
            tool_results: None,
        });

        let mut tool_call_count = 0;
        let mut all_tool_calls: Vec<AgentToolCall> = Vec::new();
        let mut execution_events: Vec<AgentExecutionEvent> = Vec::new();
        let mut session_prompt_tokens_total = 0u64;
        let mut session_completion_tokens_total = 0u64;
        let mut session_tokens_total = 0u64;
        self.emit_event(
            &mut session_state,
            &mut execution_events,
            AgentExecutionEvent {
                kind: AgentExecutionEventKind::SessionStatus,
                timestamp: chrono::Utc::now(),
                message: "session_running".to_string(),
                round: 0,
                tool_name: None,
                tool_call_id: None,
                duration_ms: None,
                is_error: false,
                workflow_stage: None,
                workflow_detail: None,
                workflow_stage_index: None,
                workflow_stage_total: None,
            },
        )
        .await;
        self.emit_workflow_stage(
            &mut session_state,
            &mut execution_events,
            0,
            AgentWorkflowStage::Planning,
            "build_prompt_and_context",
        )
        .await;
        let mut round = 0usize;
        let (final_content, final_verification) = loop {
            round += 1;

            // 检查工具调用次数
            if tool_call_count >= self.config.max_tool_calls {
                warn!("Max tool calls exceeded: {}", tool_call_count);
                self.emit_event(
                    &mut session_state,
                    &mut execution_events,
                    AgentExecutionEvent {
                        kind: AgentExecutionEventKind::Error,
                        timestamp: chrono::Utc::now(),
                        message: format!("max_tool_calls_exceeded: {}", self.config.max_tool_calls),
                        round,
                        tool_name: None,
                        tool_call_id: None,
                        duration_ms: None,
                        is_error: true,
                        workflow_stage: None,
                        workflow_detail: None,
                        workflow_stage_index: None,
                        workflow_stage_total: None,
                    },
                )
                .await;
                self.save_session(session_state).await;
                return Ok(AgentResponse {
                    session_id: session.id.clone(),
                    content: format!(
                        "I've reached the maximum number of tool calls ({}). Please review my progress and provide further guidance.",
                        self.config.max_tool_calls
                    ),
                    tool_calls: all_tool_calls,
                    is_complete: false,
                    needs_input: true,
                    verification_result: None,
                    execution_events,
                });
            }

            self.emit_workflow_stage(
                &mut session_state,
                &mut execution_events,
                round,
                AgentWorkflowStage::Executing,
                "llm_round_start",
            )
            .await;
            self.emit_event(
                &mut session_state,
                &mut execution_events,
                AgentExecutionEvent {
                    kind: AgentExecutionEventKind::StepStart,
                    timestamp: chrono::Utc::now(),
                    message: format!("llm_round_{}_start", round),
                    round,
                    tool_name: None,
                    tool_call_id: None,
                    duration_ms: None,
                    is_error: false,
                    workflow_stage: None,
                    workflow_detail: None,
                    workflow_stage_index: None,
                    workflow_stage_total: None,
                },
            )
            .await;

            // 调用 LLM
            let tool_schemas = self.tool_executor.tool_schemas();
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
                tools: if tool_schemas.is_empty() {
                    None
                } else {
                    Some(tool_schemas)
                },
            };
            let llm_started = Instant::now();

            let response = self
                .provider
                .complete(&llm_request)
                .await
                .map_err(|e| AgentError::LlmError(e.to_string()))?;
            let usage = response
                .usage
                .clone()
                .unwrap_or_else(|| self.provider.estimate_tokens(&llm_request));
            let usage_estimated = response.usage.is_none();
            session_prompt_tokens_total += usage.prompt_tokens as u64;
            session_completion_tokens_total += usage.completion_tokens as u64;
            session_tokens_total += usage.total_tokens as u64;
            self.emit_token_usage(
                &mut session_state,
                &mut execution_events,
                round,
                usage,
                session_prompt_tokens_total,
                session_completion_tokens_total,
                session_tokens_total,
                usage_estimated,
            )
            .await;

            // 获取助手响应
            let assistant_message = response
                .choices
                .first()
                .ok_or_else(|| AgentError::LlmError("No response from LLM".to_string()))?
                .message
                .clone();
            self.emit_event(
                &mut session_state,
                &mut execution_events,
                AgentExecutionEvent {
                    kind: AgentExecutionEventKind::StepFinish,
                    timestamp: chrono::Utc::now(),
                    message: format!("llm_round_{}_finish", round),
                    round,
                    tool_name: None,
                    tool_call_id: None,
                    duration_ms: Some(llm_started.elapsed().as_millis() as u64),
                    is_error: false,
                    workflow_stage: None,
                    workflow_detail: None,
                    workflow_stage_index: None,
                    workflow_stage_total: None,
                },
            )
            .await;

            // 检查是否有工具调用
            if let Some(ref tool_calls) = assistant_message.tool_calls {
                if !tool_calls.is_empty() {
                    self.emit_workflow_stage(
                        &mut session_state,
                        &mut execution_events,
                        round,
                        AgentWorkflowStage::Discovery,
                        "tool_calls_planned",
                    )
                    .await;
                    if !assistant_message.content.trim().is_empty() {
                        self.emit_event(
                            &mut session_state,
                            &mut execution_events,
                            AgentExecutionEvent {
                                kind: AgentExecutionEventKind::Reasoning,
                                timestamp: chrono::Utc::now(),
                                message: truncate_for_event(&assistant_message.content, 300),
                                round,
                                tool_name: None,
                                tool_call_id: None,
                                duration_ms: None,
                                is_error: false,
                                workflow_stage: None,
                                workflow_detail: None,
                                workflow_stage_index: None,
                                workflow_stage_total: None,
                            },
                        )
                        .await;
                    } else {
                        self.emit_event(
                            &mut session_state,
                            &mut execution_events,
                            AgentExecutionEvent {
                                kind: AgentExecutionEventKind::Reasoning,
                                timestamp: chrono::Utc::now(),
                                message: summarize_tool_calls(tool_calls),
                                round,
                                tool_name: None,
                                tool_call_id: None,
                                duration_ms: None,
                                is_error: false,
                                workflow_stage: None,
                                workflow_detail: None,
                                workflow_stage_index: None,
                                workflow_stage_total: None,
                            },
                        )
                        .await;
                    }
                    let session_tool_calls: Vec<AgentToolCall> = tool_calls
                        .iter()
                        .map(|tc| AgentToolCall {
                            name: tc.function.name.clone(),
                            arguments: tc.function.arguments.clone(),
                            id: tc.id.clone(),
                        })
                        .collect();
                    session_state.add_message(AgentMessage {
                        role: MessageRole::Assistant,
                        content: assistant_message.content.clone(),
                        timestamp: chrono::Utc::now(),
                        tool_calls: Some(session_tool_calls.clone()),
                        tool_results: None,
                    });
                    for tc in &session_tool_calls {
                        session_state.record_tool_call(&tc.name);
                    }

                    // 执行工具调用
                    let tool_results = self
                        .execute_tool_calls(
                            tool_calls,
                            round,
                            &mut execution_events,
                            &mut session_state,
                        )
                        .await?;

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
                            // We use `name` to carry tool_call_id for provider adapters.
                            name: Some(result.tool_call_id.clone()),
                            tool_calls: None,
                        });
                        session_state.add_message(AgentMessage {
                            role: MessageRole::Tool,
                            content: result.content.clone(),
                            timestamp: chrono::Utc::now(),
                            tool_calls: None,
                            tool_results: Some(vec![result.content.clone()]),
                        });
                    }

                    // 继续循环
                    continue;
                }
            }

            // 没有工具调用，获取最终内容
            let final_content = assistant_message.content.clone();
            if !final_content.trim().is_empty() {
                self.emit_event(
                    &mut session_state,
                    &mut execution_events,
                    AgentExecutionEvent {
                        kind: AgentExecutionEventKind::Text,
                        timestamp: chrono::Utc::now(),
                        message: truncate_for_event(&final_content, 300),
                        round,
                        tool_name: None,
                        tool_call_id: None,
                        duration_ms: None,
                        is_error: false,
                        workflow_stage: None,
                        workflow_detail: None,
                        workflow_stage_index: None,
                        workflow_stage_total: None,
                    },
                )
                .await;
            }
            session_state.add_message(AgentMessage {
                role: MessageRole::Assistant,
                content: final_content.clone(),
                timestamp: chrono::Utc::now(),
                tool_calls: None,
                tool_results: None,
            });

            // 如果启用了自动验证且有活跃任务，执行验证
            let verification_result = if self.config.auto_verify {
                if let Some(task_id) = active_task_id {
                    self.emit_workflow_stage(
                        &mut session_state,
                        &mut execution_events,
                        round,
                        AgentWorkflowStage::Verifying,
                        "quality_gate_and_task_verifier",
                    )
                    .await;
                    self.emit_event(
                        &mut session_state,
                        &mut execution_events,
                        AgentExecutionEvent {
                            kind: AgentExecutionEventKind::Verification,
                            timestamp: chrono::Utc::now(),
                            message: format!("verify_task: {}", task_id),
                            round,
                            tool_name: None,
                            tool_call_id: None,
                            duration_ms: None,
                            is_error: false,
                            workflow_stage: None,
                            workflow_detail: None,
                            workflow_stage_index: None,
                            workflow_stage_total: None,
                        },
                    )
                    .await;
                    self.verifier.verify_and_track(&task_id).await.ok()
                } else {
                    None
                }
            } else {
                None
            };

            // 检查是否需要继续
            let needs_continuation = match verification_result {
                Some(VerificationResult::Incomplete { .. })
                | Some(VerificationResult::QualityGateFailed { .. }) => true,
                _ => false,
            };

            if needs_continuation {
                // 添加反馈消息并继续
                let feedback = self
                    .verifier
                    .generate_continuation_prompt(verification_result.as_ref().unwrap());

                messages.push(Message {
                    role: MessageRole::System,
                    content: feedback,
                    name: None,
                    tool_calls: None,
                });
                session_state.add_message(AgentMessage {
                    role: MessageRole::System,
                    content: self
                        .verifier
                        .generate_feedback_message(verification_result.as_ref().unwrap()),
                    timestamp: chrono::Utc::now(),
                    tool_calls: None,
                    tool_results: None,
                });

                // 继续循环
                continue;
            }

            // 完成
            self.emit_workflow_stage(
                &mut session_state,
                &mut execution_events,
                round,
                AgentWorkflowStage::Completing,
                "finalize_response_and_idle",
            )
            .await;
            self.emit_event(
                &mut session_state,
                &mut execution_events,
                AgentExecutionEvent {
                    kind: AgentExecutionEventKind::SessionStatus,
                    timestamp: chrono::Utc::now(),
                    message: "session_idle".to_string(),
                    round,
                    tool_name: None,
                    tool_call_id: None,
                    duration_ms: None,
                    is_error: false,
                    workflow_stage: None,
                    workflow_detail: None,
                    workflow_stage_index: None,
                    workflow_stage_total: None,
                },
            )
            .await;
            break (final_content, verification_result);
        };

        self.save_session(session_state).await;

        Ok(AgentResponse {
            session_id: session.id,
            content: final_content,
            tool_calls: all_tool_calls,
            is_complete: true,
            needs_input: false,
            verification_result: final_verification,
            execution_events,
        })
    }

    /// 构建消息列表
    async fn build_messages(
        &self,
        session: &AgentSession,
        user_message: &Message,
        active_task_id: Option<TaskId>,
        working_dir: Option<std::path::PathBuf>,
        working_memory: Option<crate::WorkingMemory>,
    ) -> Result<Vec<Message>, AgentError> {
        let mut messages = Vec::new();

        // 构建系统提示词
        let available_tools = self.tool_executor.tool_schemas();
        let working_memory_injector = self.build_working_memory_injector(
            session,
            active_task_id.clone(),
            working_dir.clone(),
            working_memory,
        );
        let prompt_context = EnhancedPromptContext {
            available_tools,
            active_task_id,
            working_dir,
            working_memory: Some(working_memory_injector),
            invariants: None,
            lineage: None,
            context_patterns: Vec::new(),
        };

        let system_prompt = if let Some(ref template) = self.config.system_prompt_template {
            template.clone()
        } else {
            build_enhanced_prompt(&prompt_context)
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

    fn build_working_memory_injector(
        &self,
        session: &AgentSession,
        active_task_id: Option<TaskId>,
        _working_dir: Option<std::path::PathBuf>,
        working_memory: Option<crate::WorkingMemory>,
    ) -> WorkingMemoryInjector {
        let mut injector = WorkingMemoryInjector::default();

        if let Some(ref wm) = working_memory {
            injector.update(WorkingMemoryInjector::from_working_memory(wm));
            return injector;
        }

        let recent_failures: Vec<String> = session
            .messages
            .iter()
            .rev()
            .filter_map(|m| {
                let lower = m.content.to_lowercase();
                if lower.contains("error") || lower.contains("failed") || lower.contains("panic") {
                    Some(m.content.clone())
                } else {
                    None
                }
            })
            .take(5)
            .collect();

        let mut context = WorkingMemoryContext {
            abstract_summary: None,
            raw_summary: None,
            hard_constraints: Vec::new(),
            active_files: Vec::new(),
            api_surface: Vec::new(),
            recent_failures,
            current_task: None,
            custom: std::collections::HashMap::new(),
        };

        if let Some(task_id) = active_task_id {
            context.current_task = Some(TaskContext {
                task_id: task_id.to_string(),
                task_title: "active task".to_string(),
                current_step: "continue execution".to_string(),
                completed_steps: Vec::new(),
            });
            context.raw_summary = Some("Task-scoped execution context active".to_string());
        }

        if !context.recent_failures.is_empty() {
            context.abstract_summary = Some("Recent failures detected in this session".to_string());
        }

        injector.update(context);
        injector
    }

    /// 执行工具调用
    async fn execute_tool_calls(
        &self,
        tool_calls: &[ToolCall],
        round: usize,
        execution_events: &mut Vec<AgentExecutionEvent>,
        session_state: &mut AgentSession,
    ) -> Result<Vec<LlmToolResult>, AgentError> {
        let mut results = Vec::new();

        for tool_call in tool_calls {
            let function = &tool_call.function;
            let tool_name = &function.name;

            info!("Executing tool: {}", tool_name);
            self.emit_event(
                session_state,
                execution_events,
                AgentExecutionEvent {
                    kind: AgentExecutionEventKind::ToolCallStart,
                    timestamp: chrono::Utc::now(),
                    message: format!(
                        "tool_call_start: {} | args_preview: {}",
                        tool_name,
                        compact_preview(&function.arguments, 200)
                    ),
                    round,
                    tool_name: Some(tool_name.clone()),
                    tool_call_id: Some(tool_call.id.clone()),
                    duration_ms: None,
                    is_error: false,
                    workflow_stage: None,
                    workflow_detail: None,
                    workflow_stage_index: None,
                    workflow_stage_total: None,
                },
            )
            .await;
            let started = Instant::now();

            // 执行工具
            let tool_result = match self
                .tool_executor
                .execute_tool(tool_name, &function.arguments)
                .await
            {
                Ok(content) => LlmToolResult {
                    tool_call_id: tool_call.id.clone(),
                    content,
                    is_error: false,
                },
                Err(e) => {
                    if let AgentError::PermissionDenied(message) = &e {
                        self.emit_event(
                            session_state,
                            execution_events,
                            AgentExecutionEvent {
                                kind: AgentExecutionEventKind::PermissionAsked,
                                timestamp: chrono::Utc::now(),
                                message: format!("permission_asked: {}", message),
                                round,
                                tool_name: Some(tool_name.clone()),
                                tool_call_id: Some(tool_call.id.clone()),
                                duration_ms: None,
                                is_error: true,
                                workflow_stage: None,
                                workflow_detail: None,
                                workflow_stage_index: None,
                                workflow_stage_total: None,
                            },
                        )
                        .await;
                    }
                    warn!("Tool {} execution failed: {}", tool_name, e);
                    LlmToolResult {
                        tool_call_id: tool_call.id.clone(),
                        content: format!("Error: {}", e),
                        is_error: true,
                    }
                }
            };
            self.emit_event(
                session_state,
                execution_events,
                AgentExecutionEvent {
                    kind: AgentExecutionEventKind::ToolCallEnd,
                    timestamp: chrono::Utc::now(),
                    message: format!(
                        "tool_call_end: {} ({}) | args_preview: {} | result_preview: {}",
                        tool_name,
                        if tool_result.is_error { "error" } else { "ok" },
                        compact_preview(&function.arguments, 200),
                        compact_preview(&tool_result.content, 200)
                    ),
                    round,
                    tool_name: Some(tool_name.clone()),
                    tool_call_id: Some(tool_call.id.clone()),
                    duration_ms: Some(started.elapsed().as_millis() as u64),
                    is_error: tool_result.is_error,
                    workflow_stage: None,
                    workflow_detail: None,
                    workflow_stage_index: None,
                    workflow_stage_total: None,
                },
            )
            .await;

            results.push(tool_result);
        }

        Ok(results)
    }
}

fn truncate_for_event(content: &str, max: usize) -> String {
    let trimmed = content.trim();
    if trimmed.len() <= max {
        return trimmed.to_string();
    }
    let mut out = trimmed.chars().take(max).collect::<String>();
    out.push_str("...");
    out
}

fn compact_preview(content: &str, max: usize) -> String {
    let one_line = content
        .replace('\n', " ")
        .replace('\r', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    truncate_for_event(&one_line, max)
}

fn summarize_tool_calls(tool_calls: &[ToolCall]) -> String {
    let mut parts = Vec::new();
    for call in tool_calls.iter().take(3) {
        let arg = compact_preview(&call.function.arguments, 60);
        parts.push(format!("{}({})", call.function.name, arg));
    }
    let mut summary = format!("planning tool calls: {}", parts.join(", "));
    if tool_calls.len() > 3 {
        summary.push_str(&format!(", ... +{} more", tool_calls.len() - 3));
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::provider::{
        Choice, CompletionResponse, ModelInfo, ModelPermission, ProviderConfig, ProviderType,
        ToolCallFunction, Usage,
    };
    use std::collections::VecDeque;
    use tokio::sync::Mutex as TokioMutex;

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
            working_memory: None,
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
            execution_events: vec![],
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

    struct ScriptedProvider {
        config: ProviderConfig,
        responses: Arc<TokioMutex<VecDeque<CompletionResponse>>>,
        requests: Arc<TokioMutex<Vec<CompletionRequest>>>,
    }

    impl ScriptedProvider {
        fn new(responses: Vec<CompletionResponse>) -> Self {
            Self {
                config: ProviderConfig {
                    name: "mock".to_string(),
                    provider_type: ProviderType::OpenAi,
                    api_key: "test".to_string(),
                    base_url: None,
                    organization: None,
                    default_model: "mock-model".to_string(),
                    models: vec!["mock-model".to_string()],
                    timeout_ms: 1000,
                    max_retries: 1,
                },
                responses: Arc::new(TokioMutex::new(VecDeque::from(responses))),
                requests: Arc::new(TokioMutex::new(Vec::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for ScriptedProvider {
        fn provider_type(&self) -> ProviderType {
            ProviderType::OpenAi
        }

        fn name(&self) -> &str {
            "mock-provider"
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
            Ok(vec![ModelInfo {
                id: "mock-model".to_string(),
                object: "model".to_string(),
                created: 0,
                owned_by: "test".to_string(),
                permission: vec![ModelPermission {
                    id: "perm".to_string(),
                    object: "permission".to_string(),
                    created: 0,
                    allow_create_engine: true,
                    allow_sampling: true,
                    allow_logprobs: false,
                    allow_search_indices: false,
                    allow_view: true,
                    allow_fine_tuning: false,
                    organization: "test".to_string(),
                    group: None,
                    is_blocking: false,
                }],
            }])
        }

        async fn complete(
            &self,
            request: &CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            self.requests.lock().await.push(request.clone());
            self.responses
                .lock()
                .await
                .pop_front()
                .ok_or_else(|| ProviderError::InvalidRequest {
                    message: "no scripted response".to_string(),
                })
        }

        async fn complete_streaming(
            &self,
            _request: &CompletionRequest,
            _handler: &Arc<dyn StreamHandler>,
        ) -> Result<(), ProviderError> {
            Ok(())
        }

        fn estimate_tokens(&self, _request: &CompletionRequest) -> Usage {
            Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            }
        }

        async fn is_model_available(&self, _model: &str) -> bool {
            true
        }

        fn config(&self) -> &ProviderConfig {
            &self.config
        }
    }

    struct MockToolExecutor {
        calls: Arc<TokioMutex<Vec<String>>>,
    }

    impl MockToolExecutor {
        fn new() -> Self {
            Self {
                calls: Arc::new(TokioMutex::new(Vec::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl ToolExecutor for MockToolExecutor {
        async fn execute_tool(&self, name: &str, _arguments: &str) -> Result<String, AgentError> {
            self.calls.lock().await.push(name.to_string());
            Ok("ok".to_string())
        }

        fn list_tools(&self) -> Vec<String> {
            vec!["write".to_string()]
        }

        fn tool_schemas(&self) -> Vec<serde_json::Value> {
            vec![serde_json::json!({
                "type": "function",
                "function": {
                    "name": "write",
                    "description": "Write file",
                    "parameters": {"type": "object", "properties": {}}
                }
            })]
        }
    }

    struct PermissionDeniedToolExecutor;

    #[async_trait::async_trait]
    impl ToolExecutor for PermissionDeniedToolExecutor {
        async fn execute_tool(&self, _name: &str, _arguments: &str) -> Result<String, AgentError> {
            Err(AgentError::PermissionDenied(
                "Permission denied for write file".to_string(),
            ))
        }

        fn list_tools(&self) -> Vec<String> {
            vec!["write".to_string()]
        }

        fn tool_schemas(&self) -> Vec<serde_json::Value> {
            vec![serde_json::json!({
                "type": "function",
                "function": {
                    "name": "write",
                    "description": "Write file",
                    "parameters": {"type": "object", "properties": {}}
                }
            })]
        }
    }

    struct MockStorage;

    #[async_trait::async_trait]
    impl crate::ai_agent::TaskStorage for MockStorage {
        async fn get_task(
            &self,
            _id: &TaskId,
        ) -> Result<Option<crate::Task>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(None)
        }

        async fn save_memory(
            &self,
            _memory: &crate::MemoryEntry,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }

        async fn get_memory(
            &self,
            _id: &crate::MemoryId,
        ) -> Result<Option<crate::MemoryEntry>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(None)
        }
    }

    struct SequencedTaskStorage {
        first: crate::Task,
        second: crate::Task,
        calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl crate::ai_agent::TaskStorage for SequencedTaskStorage {
        async fn get_task(
            &self,
            id: &TaskId,
        ) -> Result<Option<crate::Task>, Box<dyn std::error::Error + Send + Sync>> {
            if &self.first.id != id {
                return Ok(None);
            }
            let idx = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if idx == 0 {
                Ok(Some(self.first.clone()))
            } else {
                Ok(Some(self.second.clone()))
            }
        }

        async fn save_memory(
            &self,
            _memory: &crate::MemoryEntry,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }

        async fn get_memory(
            &self,
            _id: &crate::MemoryId,
        ) -> Result<Option<crate::MemoryEntry>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn test_smoke_file_tool_call_and_session_continuation() {
        let first_response = CompletionResponse {
            id: "resp-1".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: String::new(),
                    name: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "tool-1".to_string(),
                        function: ToolCallFunction {
                            name: "write".to_string(),
                            arguments: r#"{"path":"/tmp/test.txt","content":"x"}"#.to_string(),
                        },
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            }],
            usage: None,
        };

        let second_response = CompletionResponse {
            id: "resp-2".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: "File updated.".to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: None,
        };

        let third_response = CompletionResponse {
            id: "resp-3".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: "Continuing same session.".to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: None,
        };

        let provider = Arc::new(ScriptedProvider::new(vec![
            first_response,
            second_response,
            third_response,
        ]));
        let tool_executor = Arc::new(MockToolExecutor::new());
        let verifier = Arc::new(TaskVerifier::new(Arc::new(MockStorage)));
        let orchestrator = AgentOrchestrator::new(
            provider.clone(),
            tool_executor.clone(),
            verifier,
            AgentConfig::default(),
        );

        let first = orchestrator
            .process(AgentRequest {
                user_input: "write to file".to_string(),
                session_id: None,
                working_dir: None,
                role: None,
                active_task_id: None,
                working_memory: None,
            })
            .await
            .unwrap();

        assert_eq!(first.content, "File updated.");
        assert_eq!(tool_executor.calls.lock().await.len(), 1);
        assert!(first.execution_events.iter().any(|e| {
            e.kind == AgentExecutionEventKind::ToolCallStart
                && e.tool_name.as_deref() == Some("write")
        }));
        assert!(
            first
                .execution_events
                .iter()
                .any(|e| e.kind == AgentExecutionEventKind::Reasoning)
        );
        assert!(
            first
                .execution_events
                .iter()
                .any(|e| e.kind == AgentExecutionEventKind::ToolCallEnd)
        );
        assert!(first.execution_events.iter().any(|e| {
            e.kind == AgentExecutionEventKind::ToolCallEnd && e.message.contains("result_preview:")
        }));

        let session_id = first.session_id.clone();
        let second = orchestrator
            .process(AgentRequest {
                user_input: "continue".to_string(),
                session_id: Some(session_id),
                working_dir: None,
                role: None,
                active_task_id: None,
                working_memory: None,
            })
            .await
            .unwrap();

        assert_eq!(second.content, "Continuing same session.");
        let captured = provider.requests.lock().await;
        let last_request = captured.last().unwrap();
        assert!(
            last_request
                .messages
                .iter()
                .any(|m| m.content.contains("File updated."))
        );

        let replay = orchestrator
            .get_session_execution_events(&first.session_id, Some(3))
            .await
            .unwrap();
        assert!(!replay.is_empty());
        assert!(replay.len() <= 3);
    }

    #[tokio::test]
    async fn test_quality_gate_feedback_loop_continues() {
        let first_response = CompletionResponse {
            id: "resp-a".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: "First answer".to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: None,
        };
        let second_response = CompletionResponse {
            id: "resp-b".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: "Second answer after feedback".to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: None,
        };

        let provider = Arc::new(ScriptedProvider::new(vec![first_response, second_response]));
        let tool_executor = Arc::new(MockToolExecutor::new());

        let pending_task = crate::Task::new(
            "needs verification".to_string(),
            "pending task".to_string(),
            crate::AgentRole::Implementer,
        );
        let mut completed_task = pending_task.clone();
        completed_task.state = crate::TaskState::Completed;
        let task_id = pending_task.id;
        let verifier = Arc::new(TaskVerifier::new(Arc::new(SequencedTaskStorage {
            first: pending_task,
            second: completed_task,
            calls: std::sync::atomic::AtomicUsize::new(0),
        })));

        let orchestrator = AgentOrchestrator::new(
            provider.clone(),
            tool_executor,
            verifier,
            AgentConfig::default(),
        );

        let response = orchestrator
            .process(AgentRequest {
                user_input: "do work".to_string(),
                session_id: None,
                working_dir: None,
                role: None,
                active_task_id: Some(task_id),
                working_memory: None,
            })
            .await
            .unwrap();

        assert_eq!(response.content, "Second answer after feedback");
        let requests = provider.requests.lock().await;
        assert_eq!(requests.len(), 2);
        let second_req = &requests[1];
        assert!(
            second_req
                .messages
                .iter()
                .any(|m| m.content.contains("Task verification failed"))
        );
    }

    #[tokio::test]
    async fn test_workflow_stage_and_token_usage_events_emitted() {
        let response = CompletionResponse {
            id: "resp-workflow-1".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: "done".to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: Some(Usage {
                prompt_tokens: 11,
                completion_tokens: 7,
                total_tokens: 18,
            }),
        };
        let provider = Arc::new(ScriptedProvider::new(vec![response]));
        let tool_executor = Arc::new(MockToolExecutor::new());
        let verifier = Arc::new(TaskVerifier::new(Arc::new(MockStorage)));
        let orchestrator =
            AgentOrchestrator::new(provider, tool_executor, verifier, AgentConfig::default());

        let result = orchestrator
            .process(AgentRequest {
                user_input: "ping".to_string(),
                session_id: None,
                working_dir: None,
                role: None,
                active_task_id: None,
                working_memory: None,
            })
            .await
            .unwrap();

        let stage_messages = result
            .execution_events
            .iter()
            .filter(|event| event.kind == AgentExecutionEventKind::WorkflowStage)
            .map(|event| event.message.clone())
            .collect::<Vec<_>>();
        let planning_idx = stage_messages
            .iter()
            .position(|message| message.contains("workflow_stage: planning"))
            .expect("planning stage");
        let executing_idx = stage_messages
            .iter()
            .position(|message| message.contains("workflow_stage: executing"))
            .expect("executing stage");
        let completing_idx = stage_messages
            .iter()
            .position(|message| message.contains("workflow_stage: completing"))
            .expect("completing stage");
        assert!(planning_idx < executing_idx);
        assert!(executing_idx < completing_idx);

        let token_event = result
            .execution_events
            .iter()
            .find(|event| event.kind == AgentExecutionEventKind::TokenUsage)
            .expect("token usage event");
        assert!(token_event.message.contains("source=provider"));
        assert!(token_event.message.contains("prompt=11"));
        assert!(token_event.message.contains("completion=7"));
        assert!(token_event.message.contains("total=18"));
        assert!(token_event.message.contains("session_total=18"));
    }

    #[tokio::test]
    async fn test_token_usage_falls_back_to_estimated_when_provider_missing_usage() {
        let response = CompletionResponse {
            id: "resp-token-fallback-1".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: "done".to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: None,
        };
        let provider = Arc::new(ScriptedProvider::new(vec![response]));
        let tool_executor = Arc::new(MockToolExecutor::new());
        let verifier = Arc::new(TaskVerifier::new(Arc::new(MockStorage)));
        let orchestrator =
            AgentOrchestrator::new(provider, tool_executor, verifier, AgentConfig::default());

        let result = orchestrator
            .process(AgentRequest {
                user_input: "ping".to_string(),
                session_id: None,
                working_dir: None,
                role: None,
                active_task_id: None,
                working_memory: None,
            })
            .await
            .unwrap();

        let token_event = result
            .execution_events
            .iter()
            .find(|event| event.kind == AgentExecutionEventKind::TokenUsage)
            .expect("token usage event");
        assert!(token_event.message.contains("source=estimated"));
        assert!(token_event.message.contains("prompt=1"));
        assert!(token_event.message.contains("completion=1"));
        assert!(token_event.message.contains("total=2"));
        assert!(token_event.message.contains("session_total=2"));
    }

    #[tokio::test]
    async fn test_permission_denied_emits_permission_asked_event() {
        let first_response = CompletionResponse {
            id: "resp-perm-1".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: String::new(),
                    name: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "tool-perm-1".to_string(),
                        function: ToolCallFunction {
                            name: "write".to_string(),
                            arguments: r#"{"path":"/tmp/test.txt","content":"x"}"#.to_string(),
                        },
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            }],
            usage: None,
        };
        let second_response = CompletionResponse {
            id: "resp-perm-2".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: "Cannot write without permission.".to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: None,
        };

        let provider = Arc::new(ScriptedProvider::new(vec![first_response, second_response]));
        let verifier = Arc::new(TaskVerifier::new(Arc::new(MockStorage)));
        let orchestrator = AgentOrchestrator::new(
            provider,
            Arc::new(PermissionDeniedToolExecutor),
            verifier,
            AgentConfig::default(),
        );

        let response = orchestrator
            .process(AgentRequest {
                user_input: "write to file".to_string(),
                session_id: None,
                working_dir: None,
                role: None,
                active_task_id: None,
                working_memory: None,
            })
            .await
            .unwrap();

        assert!(response.execution_events.iter().any(|e| {
            e.kind == AgentExecutionEventKind::PermissionAsked
                && e.tool_name.as_deref() == Some("write")
                && e.is_error
        }));
    }

    #[tokio::test]
    async fn test_multiround_replay_contains_permission_asked() {
        let first_response = CompletionResponse {
            id: "resp-multi-1".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: String::new(),
                    name: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "tool-multi-1".to_string(),
                        function: ToolCallFunction {
                            name: "write".to_string(),
                            arguments: r#"{"path":"/tmp/test.txt","content":"x"}"#.to_string(),
                        },
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            }],
            usage: None,
        };
        let second_response = CompletionResponse {
            id: "resp-multi-2".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: "Need permission before writing.".to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: None,
        };
        let third_response = CompletionResponse {
            id: "resp-multi-3".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: "Second round status update.".to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: None,
        };

        let provider = Arc::new(ScriptedProvider::new(vec![
            first_response,
            second_response,
            third_response,
        ]));
        let verifier = Arc::new(TaskVerifier::new(Arc::new(MockStorage)));
        let orchestrator = AgentOrchestrator::new(
            provider,
            Arc::new(PermissionDeniedToolExecutor),
            verifier,
            AgentConfig::default(),
        );

        let first = orchestrator
            .process(AgentRequest {
                user_input: "write once".to_string(),
                session_id: None,
                working_dir: None,
                role: None,
                active_task_id: None,
                working_memory: None,
            })
            .await
            .unwrap();
        let second = orchestrator
            .process(AgentRequest {
                user_input: "continue".to_string(),
                session_id: Some(first.session_id.clone()),
                working_dir: None,
                role: None,
                active_task_id: None,
                working_memory: None,
            })
            .await
            .unwrap();
        assert_eq!(second.content, "Second round status update.");

        let replay = orchestrator
            .get_session_execution_events(&first.session_id, None)
            .await
            .unwrap();
        assert!(
            replay
                .iter()
                .any(|e| e.kind == AgentExecutionEventKind::PermissionAsked)
        );
    }

    #[tokio::test]
    async fn test_subscribe_execution_events_broadcasts_session_events() {
        let response = CompletionResponse {
            id: "resp-live-1".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: "done".to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: None,
        };

        let provider = Arc::new(ScriptedProvider::new(vec![response]));
        let tool_executor = Arc::new(MockToolExecutor::new());
        let verifier = Arc::new(TaskVerifier::new(Arc::new(MockStorage)));
        let orchestrator =
            AgentOrchestrator::new(provider, tool_executor, verifier, AgentConfig::default());
        let mut rx = orchestrator.subscribe_execution_events();

        let result = orchestrator
            .process(AgentRequest {
                user_input: "ping".to_string(),
                session_id: None,
                working_dir: None,
                role: None,
                active_task_id: None,
                working_memory: None,
            })
            .await
            .unwrap();

        let mut events = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        while tokio::time::Instant::now() < deadline {
            match rx.try_recv() {
                Ok(event) => {
                    events.push(event);
                    if events.len() >= 2 {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
            }
        }

        assert!(!events.is_empty());
        assert!(events.iter().all(|e| e.session_id == result.session_id));
        assert!(
            events
                .iter()
                .any(|e| e.event.kind == AgentExecutionEventKind::SessionStatus)
        );
    }
}
