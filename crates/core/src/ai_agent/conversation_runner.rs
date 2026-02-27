//! Conversation loop runner — manages the multi-round LLM ↔ tool execution loop.
//!
//! Extracted from `orchestrator.rs` to reduce god-object complexity.
//! Holds cloned Arc references to shared resources and runs the main
//! conversation loop (`run_main_loop`) plus tool execution (`execute_tool_calls`).

use super::helpers::{
    MAX_CONVERSATION_MESSAGES, compact_preview, is_confirmation_permission_error,
    sanitize_tool_output, summarize_tool_calls, truncate_for_event, truncate_messages,
};
use super::orchestrator::{AgentConfig, AgentResponse, ToolExecutor};
use super::{
    AgentError, AgentExecutionEvent, AgentExecutionEventKind, AgentMessage, AgentSession,
    AgentSessionExecutionEvent, AgentToolCall, AgentWorkflowStage, TaskVerifier,
    VerificationResult, prompt_builder, session_store::SessionStore,
};
use crate::TaskId;
use crate::llm::provider::{
    CompletionRequest, LlmProvider, Message, MessageRole, ToolCall as LlmToolCall,
    ToolResult as LlmToolResult,
};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, broadcast};
use tracing::{info, warn};

/// Accumulated token counts for the current session run.
struct SessionTokenTotals {
    prompt: u64,
    completion: u64,
    total: u64,
}

/// Bundles shared resources needed to drive a conversation loop.
///
/// Created per-request from `AgentOrchestrator` fields (cheap Arc clones).
pub(crate) struct ConversationRunner {
    provider: Arc<dyn LlmProvider>,
    tool_executor: Arc<dyn ToolExecutor>,
    verifier: Arc<TaskVerifier>,
    config: AgentConfig,
    event_tx: broadcast::Sender<AgentSessionExecutionEvent>,
    store: Arc<Mutex<SessionStore>>,
}

impl ConversationRunner {
    pub(crate) fn new(
        provider: Arc<dyn LlmProvider>,
        tool_executor: Arc<dyn ToolExecutor>,
        verifier: Arc<TaskVerifier>,
        config: AgentConfig,
        event_tx: broadcast::Sender<AgentSessionExecutionEvent>,
        store: Arc<Mutex<SessionStore>>,
    ) -> Self {
        Self {
            provider,
            tool_executor,
            verifier,
            config,
            event_tx,
            store,
        }
    }

    // ── event helpers ───────────────────────────────────────────────

    async fn emit_event(
        &self,
        session_state: &mut AgentSession,
        execution_events: &mut Vec<AgentExecutionEvent>,
        event: AgentExecutionEvent,
    ) {
        if let Err(e) = self.event_tx.send(AgentSessionExecutionEvent {
            session_id: session_state.id.clone(),
            event: event.clone(),
        }) {
            tracing::warn!(
                receivers = self.event_tx.receiver_count(),
                "Event broadcast failed: {}",
                e
            );
        }
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
        session_totals: &SessionTokenTotals,
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
                    session_totals.prompt,
                    session_totals.completion,
                    session_totals.total
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

    // ── delegation helpers ──────────────────────────────────────────

    async fn save_session(&self, session: AgentSession) {
        self.store.lock().await.save_session(session);
    }

    async fn build_messages(
        &self,
        session: &AgentSession,
        user_message: &Message,
        active_task_id: Option<TaskId>,
        working_dir: Option<std::path::PathBuf>,
        working_memory: Option<crate::WorkingMemory>,
    ) -> Result<Vec<Message>, AgentError> {
        prompt_builder::build_messages(
            session,
            user_message,
            active_task_id,
            working_dir,
            working_memory,
            &self.config.system_prompt_template,
            self.tool_executor.tool_schemas(),
        )
    }

    // ── main conversation loop ──────────────────────────────────────

    /// Run the non-streaming conversation loop.
    pub(crate) async fn run_main_loop(
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
                active_task_id,
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
            tool_call_id: None,
        });

        let mut tool_call_count = 0;
        let mut all_tool_calls: Vec<AgentToolCall> = Vec::new();
        let mut execution_events: Vec<AgentExecutionEvent> = Vec::new();
        let mut session_token_totals = SessionTokenTotals {
            prompt: 0,
            completion: 0,
            total: 0,
        };
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

            // Truncate message history to prevent unbounded growth
            truncate_messages(&mut messages, MAX_CONVERSATION_MESSAGES);

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
            session_token_totals.prompt += usage.prompt_tokens as u64;
            session_token_totals.completion += usage.completion_tokens as u64;
            session_token_totals.total += usage.total_tokens as u64;
            self.emit_token_usage(
                &mut session_state,
                &mut execution_events,
                round,
                usage,
                &session_token_totals,
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
            if let Some(ref tool_calls) = assistant_message.tool_calls
                && !tool_calls.is_empty()
            {
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
                    tool_call_id: None,
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
                    let sanitized = sanitize_tool_output(&result.content);
                    messages.push(Message {
                        role: MessageRole::Tool,
                        content: sanitized.clone(),
                        // We use `name` to carry tool_call_id for provider adapters.
                        name: Some(result.tool_call_id.clone()),
                        tool_calls: None,
                    });
                    session_state.add_message(AgentMessage {
                        role: MessageRole::Tool,
                        content: sanitized.clone(),
                        timestamp: chrono::Utc::now(),
                        tool_calls: None,
                        tool_results: Some(vec![sanitized]),
                        tool_call_id: Some(result.tool_call_id.clone()),
                    });
                }

                // 继续循环
                continue;
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
                tool_call_id: None,
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

            // 检查验证结果是否需要继续（直接解构，避免 unwrap panic）
            let needs_continuation = matches!(
                verification_result,
                Some(VerificationResult::Incomplete { .. })
                    | Some(VerificationResult::QualityGateFailed { .. })
            );

            if let (true, Some(vr)) = (needs_continuation, &verification_result) {
                // 添加反馈消息并继续
                let feedback = self.verifier.generate_continuation_prompt(vr);

                messages.push(Message {
                    role: MessageRole::System,
                    content: feedback,
                    name: None,
                    tool_calls: None,
                });
                session_state.add_message(AgentMessage {
                    role: MessageRole::System,
                    content: self.verifier.generate_feedback_message(vr),
                    timestamp: chrono::Utc::now(),
                    tool_calls: None,
                    tool_results: None,
                    tool_call_id: None,
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

    // ── tool execution ──────────────────────────────────────────────

    async fn execute_tool_calls(
        &self,
        tool_calls: &[LlmToolCall],
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

                        if is_confirmation_permission_error(message.as_str()) {
                            match self
                                .tool_executor
                                .confirm_and_retry_permission(
                                    tool_name,
                                    &function.arguments,
                                    message.as_str(),
                                )
                                .await
                            {
                                Ok(Some(content)) => {
                                    self.emit_event(
                                        session_state,
                                        execution_events,
                                        AgentExecutionEvent {
                                            kind: AgentExecutionEventKind::PermissionAsked,
                                            timestamp: chrono::Utc::now(),
                                            message: format!(
                                                "permission_asked: permission_approved: {}",
                                                message
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
                                    LlmToolResult {
                                        tool_call_id: tool_call.id.clone(),
                                        content,
                                        is_error: false,
                                    }
                                }
                                Ok(None) => {
                                    warn!("Tool {} execution failed: {}", tool_name, e);
                                    LlmToolResult {
                                        tool_call_id: tool_call.id.clone(),
                                        content: format!("Error: {}", e),
                                        is_error: true,
                                    }
                                }
                                Err(AgentError::PermissionDenied(rejected)) => {
                                    let rejected_payload = if rejected
                                        .trim_start()
                                        .starts_with("permission_rejected:")
                                    {
                                        rejected
                                    } else {
                                        format!("permission_rejected: {}", rejected)
                                    };
                                    self.emit_event(
                                        session_state,
                                        execution_events,
                                        AgentExecutionEvent {
                                            kind: AgentExecutionEventKind::PermissionAsked,
                                            timestamp: chrono::Utc::now(),
                                            message: format!(
                                                "permission_asked: {}",
                                                rejected_payload
                                            ),
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
                                    warn!(
                                        "Tool {} execution rejected after confirmation: {}",
                                        tool_name, rejected_payload
                                    );
                                    LlmToolResult {
                                        tool_call_id: tool_call.id.clone(),
                                        content: format!("Error: {}", rejected_payload),
                                        is_error: true,
                                    }
                                }
                                Err(other) => {
                                    warn!(
                                        "Tool {} execution failed after confirmation retry: {}",
                                        tool_name, other
                                    );
                                    LlmToolResult {
                                        tool_call_id: tool_call.id.clone(),
                                        content: format!("Error: {}", other),
                                        is_error: true,
                                    }
                                }
                            }
                        } else {
                            warn!("Tool {} execution failed: {}", tool_name, e);
                            LlmToolResult {
                                tool_call_id: tool_call.id.clone(),
                                content: format!("Error: {}", e),
                                is_error: true,
                            }
                        }
                    } else {
                        warn!("Tool {} execution failed: {}", tool_name, e);
                        LlmToolResult {
                            tool_call_id: tool_call.id.clone(),
                            content: format!("Error: {}", e),
                            is_error: true,
                        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_agent::TaskStorage;
    use crate::llm::provider::{
        Choice, CompletionResponse, ModelInfo, ModelPermission, ProviderConfig, ProviderType,
        StreamHandler, ToolCallFunction, Usage,
    };
    use std::collections::VecDeque;
    use tokio::sync::Mutex as TokioMutex;

    // ── test helpers (mirrors orchestrator test infrastructure) ──────

    struct ScriptedProvider {
        config: ProviderConfig,
        responses: Arc<TokioMutex<VecDeque<CompletionResponse>>>,
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
        async fn list_models(&self) -> Result<Vec<ModelInfo>, crate::llm::provider::ProviderError> {
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
            _request: &CompletionRequest,
        ) -> Result<CompletionResponse, crate::llm::provider::ProviderError> {
            self.responses.lock().await.pop_front().ok_or_else(|| {
                crate::llm::provider::ProviderError::InvalidRequest {
                    message: "no scripted response".to_string(),
                }
            })
        }
        async fn complete_streaming(
            &self,
            _request: &CompletionRequest,
            _handler: &Arc<dyn StreamHandler>,
        ) -> Result<(), crate::llm::provider::ProviderError> {
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

    struct MockStorage;

    #[async_trait::async_trait]
    impl TaskStorage for MockStorage {
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

    fn make_runner(
        provider: Arc<dyn LlmProvider>,
        tool_executor: Arc<dyn ToolExecutor>,
    ) -> ConversationRunner {
        let verifier = Arc::new(TaskVerifier::new(Arc::new(MockStorage)));
        let (event_tx, _) = broadcast::channel(256);
        ConversationRunner::new(
            provider,
            tool_executor,
            verifier,
            AgentConfig::default(),
            event_tx,
            Arc::new(Mutex::new(SessionStore::new())),
        )
    }

    // ── tests ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_emit_event_broadcasts_and_records() {
        let runner = make_runner(
            Arc::new(ScriptedProvider::new(vec![])),
            Arc::new(MockToolExecutor::new()),
        );
        let mut rx = runner.event_tx.subscribe();
        let mut session = AgentSession::new("emit-test".to_string());
        let mut events = Vec::new();

        runner
            .emit_event(
                &mut session,
                &mut events,
                AgentExecutionEvent {
                    kind: AgentExecutionEventKind::StepStart,
                    timestamp: chrono::Utc::now(),
                    message: "hello".to_string(),
                    round: 1,
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

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].message, "hello");
        // broadcast was sent
        let received = rx.try_recv().expect("should receive broadcast");
        assert_eq!(received.session_id, "emit-test");
        // session was saved to store
        let snapshot = runner.store.lock().await.session_snapshot("emit-test");
        assert!(snapshot.is_some());
    }

    #[tokio::test]
    async fn test_execute_tool_calls_success() {
        let tool_executor = Arc::new(MockToolExecutor::new());
        let runner = make_runner(
            Arc::new(ScriptedProvider::new(vec![])),
            tool_executor.clone(),
        );
        let mut session = AgentSession::new("tool-test".to_string());
        let mut events = Vec::new();

        let tool_calls = vec![LlmToolCall {
            id: "call-1".to_string(),
            function: ToolCallFunction {
                name: "write".to_string(),
                arguments: "{}".to_string(),
            },
        }];

        let results = runner
            .execute_tool_calls(&tool_calls, 1, &mut events, &mut session)
            .await
            .expect("should succeed");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_call_id, "call-1");
        assert!(!results[0].is_error);
        assert_eq!(results[0].content, "ok");
        // Should have ToolCallStart + ToolCallEnd events
        assert!(
            events
                .iter()
                .any(|e| e.kind == AgentExecutionEventKind::ToolCallStart)
        );
        assert!(
            events
                .iter()
                .any(|e| e.kind == AgentExecutionEventKind::ToolCallEnd)
        );
    }

    #[tokio::test]
    async fn test_execute_tool_calls_error_produces_error_result() {
        struct FailingExecutor;
        #[async_trait::async_trait]
        impl ToolExecutor for FailingExecutor {
            async fn execute_tool(&self, _name: &str, _args: &str) -> Result<String, AgentError> {
                Err(AgentError::ToolError("disk full".to_string()))
            }
            fn list_tools(&self) -> Vec<String> {
                vec!["write".to_string()]
            }
        }

        let runner = make_runner(
            Arc::new(ScriptedProvider::new(vec![])),
            Arc::new(FailingExecutor),
        );
        let mut session = AgentSession::new("fail-test".to_string());
        let mut events = Vec::new();

        let tool_calls = vec![LlmToolCall {
            id: "call-fail".to_string(),
            function: ToolCallFunction {
                name: "write".to_string(),
                arguments: "{}".to_string(),
            },
        }];

        let results = runner
            .execute_tool_calls(&tool_calls, 1, &mut events, &mut session)
            .await
            .expect("should return Ok with error result");

        assert_eq!(results.len(), 1);
        assert!(results[0].is_error);
        assert!(results[0].content.contains("disk full"));
    }

    #[tokio::test]
    async fn test_run_main_loop_simple_response() {
        let response = CompletionResponse {
            id: "resp-1".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: "Hello, world!".to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
        };

        let runner = make_runner(
            Arc::new(ScriptedProvider::new(vec![response])),
            Arc::new(MockToolExecutor::new()),
        );

        let session = AgentSession::new("simple-test".to_string());
        let user_msg = Message {
            role: MessageRole::User,
            content: "Hi".to_string(),
            name: None,
            tool_calls: None,
        };

        let result = runner
            .run_main_loop(session, user_msg, None, None, None)
            .await
            .expect("should succeed");

        assert_eq!(result.content, "Hello, world!");
        assert!(result.is_complete);
        assert!(!result.needs_input);
        assert!(result.tool_calls.is_empty());
        // Should have SessionStatus + WorkflowStage events
        assert!(
            result
                .execution_events
                .iter()
                .any(|e| e.kind == AgentExecutionEventKind::SessionStatus)
        );
    }

    #[tokio::test]
    async fn test_run_main_loop_max_tool_calls_exceeded() {
        // Provider always returns a tool call → will hit max_tool_calls limit
        let tool_response = || CompletionResponse {
            id: "resp".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: String::new(),
                    name: None,
                    tool_calls: Some(vec![LlmToolCall {
                        id: "tc".to_string(),
                        function: ToolCallFunction {
                            name: "write".to_string(),
                            arguments: "{}".to_string(),
                        },
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            }],
            usage: None,
        };

        // Create enough responses to exceed max (config default = 50, each has 1 tool call)
        let responses: Vec<_> = (0..60).map(|_| tool_response()).collect();
        let mut config = AgentConfig::default();
        config.max_tool_calls = 3;

        let verifier = Arc::new(TaskVerifier::new(Arc::new(MockStorage)));
        let (event_tx, _) = broadcast::channel(256);
        let runner = ConversationRunner::new(
            Arc::new(ScriptedProvider::new(responses)),
            Arc::new(MockToolExecutor::new()),
            verifier,
            config,
            event_tx,
            Arc::new(Mutex::new(SessionStore::new())),
        );

        let session = AgentSession::new("max-tc-test".to_string());
        let user_msg = Message {
            role: MessageRole::User,
            content: "do stuff".to_string(),
            name: None,
            tool_calls: None,
        };

        let result = runner
            .run_main_loop(session, user_msg, None, None, None)
            .await
            .expect("should return max-exceeded response");

        assert!(!result.is_complete);
        assert!(result.needs_input);
        assert!(result.content.contains("maximum number of tool calls"));
        assert!(
            result
                .execution_events
                .iter()
                .any(|e| e.kind == AgentExecutionEventKind::Error
                    && e.message.contains("max_tool_calls_exceeded"))
        );
    }

    #[tokio::test]
    async fn test_run_main_loop_with_tool_call_round_trip() {
        let tool_call_response = CompletionResponse {
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
                    tool_calls: Some(vec![LlmToolCall {
                        id: "tool-1".to_string(),
                        function: ToolCallFunction {
                            name: "write".to_string(),
                            arguments: r#"{"path":"test.txt"}"#.to_string(),
                        },
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            }],
            usage: None,
        };

        let final_response = CompletionResponse {
            id: "resp-2".to_string(),
            object: "chat.completion".to_string(),
            created: 0,
            model: "mock-model".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: "Done writing.".to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: None,
        };

        let tool_executor = Arc::new(MockToolExecutor::new());
        let runner = make_runner(
            Arc::new(ScriptedProvider::new(vec![
                tool_call_response,
                final_response,
            ])),
            tool_executor.clone(),
        );

        let session = AgentSession::new("roundtrip-test".to_string());
        let user_msg = Message {
            role: MessageRole::User,
            content: "write something".to_string(),
            name: None,
            tool_calls: None,
        };

        let result = runner
            .run_main_loop(session, user_msg, None, None, None)
            .await
            .expect("should succeed");

        assert_eq!(result.content, "Done writing.");
        assert!(result.is_complete);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "write");
        assert_eq!(tool_executor.calls.lock().await.len(), 1);
    }
}
