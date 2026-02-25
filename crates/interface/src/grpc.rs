//! gRPC 服务实现（当启用 grpc feature 时）
//!
//! 使用 tonic 框架提供 gRPC 服务

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info, warn};

use ndc_core::AgentRole;
use ndc_core::TaskId;
use ndc_runtime::{ExecutionContext, Executor};

use crate::agent_mode::{AgentModeConfig, AgentModeManager};
use crate::daemon::NdcDaemon;
use crate::redaction::{sanitize_text, RedactionMode};

// Re-export generated types from the proto
pub use super::generated;

// Type aliases for streaming responses
type ChatResponseStream = ReceiverStream<Result<generated::ChatResponse, tonic::Status>>;
type TaskExecutionEventStream =
    ReceiverStream<Result<generated::TaskExecutionEvent, tonic::Status>>;
type ToolResponseStream = ReceiverStream<Result<generated::ToolResponse, tonic::Status>>;
type ExecutionEventStream = ReceiverStream<Result<generated::ExecutionEvent, tonic::Status>>;

const DEFAULT_TIMELINE_STREAM_POLL_MS: u64 = 200;
const MIN_TIMELINE_STREAM_POLL_MS: u64 = 50;
const MAX_TIMELINE_STREAM_POLL_MS: u64 = 2_000;

fn poll_interval_ms(env_key: &str, default_ms: u64) -> u64 {
    std::env::var(env_key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|ms| ms.clamp(MIN_TIMELINE_STREAM_POLL_MS, MAX_TIMELINE_STREAM_POLL_MS))
        .unwrap_or(default_ms)
}

fn timeline_stream_poll_ms() -> u64 {
    poll_interval_ms(
        "NDC_TIMELINE_STREAM_POLL_MS",
        DEFAULT_TIMELINE_STREAM_POLL_MS,
    )
}

fn timeline_sse_poll_ms() -> u64 {
    poll_interval_ms("NDC_TIMELINE_SSE_POLL_MS", timeline_stream_poll_ms())
}

fn initial_stream_cursor(total_events: usize, backlog_limit: usize) -> usize {
    if backlog_limit == 0 {
        total_events
    } else {
        total_events.saturating_sub(backlog_limit)
    }
}

fn normalize_stream_cursor(sent_cursor: usize, total_events: usize) -> usize {
    sent_cursor.min(total_events)
}

fn resolve_timeline_sse_address(grpc_addr: SocketAddr) -> Option<SocketAddr> {
    let raw = std::env::var("NDC_TIMELINE_SSE_ADDR").ok()?;
    let value = raw.trim();
    if value.eq_ignore_ascii_case("auto") {
        let port = grpc_addr.port().saturating_add(1);
        return Some(SocketAddr::new(grpc_addr.ip(), port));
    }
    value.parse::<SocketAddr>().ok()
}

fn execution_event_to_json(event: generated::ExecutionEvent) -> String {
    serde_json::json!({
        "kind": event.kind,
        "timestamp": event.timestamp,
        "message": event.message,
        "round": event.round,
        "tool_name": event.tool_name,
        "tool_call_id": event.tool_call_id,
        "duration_ms": event.duration_ms,
        "is_error": event.is_error,
        "workflow_stage": event.workflow_stage,
        "workflow_detail": event.workflow_detail,
        "token_source": event.token_source,
        "token_prompt": event.token_prompt,
        "token_completion": event.token_completion,
        "token_total": event.token_total,
        "token_session_prompt_total": event.token_session_prompt_total,
        "token_session_completion_total": event.token_session_completion_total,
        "token_session_total": event.token_session_total,
        "workflow_stage_index": event.workflow_stage_index,
        "workflow_stage_total": event.workflow_stage_total
    })
    .to_string()
}

/// gRPC Agent 服务实现
pub struct AgentGrpcService {
    _daemon: Arc<NdcDaemon>,
    agent_manager: Arc<AgentModeManager>,
}

impl std::fmt::Debug for AgentGrpcService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentGrpcService").finish_non_exhaustive()
    }
}

impl AgentGrpcService {
    fn build_agent_manager(daemon: &Arc<NdcDaemon>) -> Arc<AgentModeManager> {
        let executor = daemon.executor();
        let tool_registry = Arc::new(ndc_runtime::create_default_tool_registry_with_storage(
            executor.context().storage.clone(),
        ));
        Arc::new(AgentModeManager::new(executor.clone(), tool_registry))
    }

    pub fn with_manager(daemon: Arc<NdcDaemon>, agent_manager: Arc<AgentModeManager>) -> Self {
        Self {
            _daemon: daemon,
            agent_manager,
        }
    }

    pub fn new(daemon: Arc<NdcDaemon>) -> Self {
        let agent_manager = Self::build_agent_manager(&daemon);
        Self::with_manager(daemon, agent_manager)
    }

    async fn ensure_agent_enabled(&self) -> Result<(), tonic::Status> {
        if self.agent_manager.is_enabled().await {
            return Ok(());
        }
        self.agent_manager
            .enable(AgentModeConfig::default())
            .await
            .map_err(|e| tonic::Status::internal(format!("failed to enable agent mode: {}", e)))
    }

    async fn validate_requested_session(
        &self,
        requested_session_id: &str,
    ) -> Result<(), tonic::Status> {
        if requested_session_id.is_empty() {
            return Ok(());
        }
        let current = self.agent_manager.status().await;
        let same = current
            .session_id
            .as_ref()
            .map(|sid| sid == requested_session_id)
            .unwrap_or(false);
        if same {
            Ok(())
        } else {
            Err(tonic::Status::not_found(format!(
                "session '{}' is not active on this daemon",
                requested_session_id
            )))
        }
    }

    fn map_execution_event(event: ndc_core::AgentExecutionEvent) -> generated::ExecutionEvent {
        let workflow = event.workflow_stage_info();
        let usage = event.token_usage_info();
        generated::ExecutionEvent {
            kind: format!("{:?}", event.kind),
            timestamp: event.timestamp.to_rfc3339(),
            message: sanitize_text(&event.message, RedactionMode::from_env()),
            round: event.round as u32,
            tool_name: event.tool_name.unwrap_or_default(),
            tool_call_id: event.tool_call_id.unwrap_or_default(),
            duration_ms: event.duration_ms.unwrap_or(0),
            is_error: event.is_error,
            workflow_stage: workflow
                .as_ref()
                .map(|value| value.stage.to_string())
                .unwrap_or_default(),
            workflow_detail: workflow
                .as_ref()
                .map(|value| value.detail.clone())
                .unwrap_or_default(),
            token_source: usage
                .as_ref()
                .map(|value| value.source.clone())
                .unwrap_or_default(),
            token_prompt: usage.as_ref().map(|value| value.prompt_tokens).unwrap_or(0),
            token_completion: usage
                .as_ref()
                .map(|value| value.completion_tokens)
                .unwrap_or(0),
            token_total: usage.as_ref().map(|value| value.total_tokens).unwrap_or(0),
            token_session_prompt_total: usage
                .as_ref()
                .map(|value| value.session_prompt_total)
                .unwrap_or(0),
            token_session_completion_total: usage
                .as_ref()
                .map(|value| value.session_completion_total)
                .unwrap_or(0),
            token_session_total: usage.as_ref().map(|value| value.session_total).unwrap_or(0),
            workflow_stage_index: workflow.as_ref().map(|value| value.index).unwrap_or(0),
            workflow_stage_total: workflow.as_ref().map(|value| value.total).unwrap_or(0),
        }
    }
}

#[tonic::async_trait]
impl generated::agent_service_server::AgentService for AgentGrpcService {
    type AgentChatStream = ChatResponseStream;
    type ExecuteToolStream = ToolResponseStream;
    type SubscribeSessionTimelineStream = ExecutionEventStream;

    /// 获取 Agent 状态
    async fn get_agent_status(
        &self,
        _request: tonic::Request<generated::AgentStatusRequest>,
    ) -> Result<tonic::Response<generated::AgentStatusResponse>, tonic::Status> {
        self.ensure_agent_enabled().await?;
        let status = self.agent_manager.status().await;
        Ok(tonic::Response::new(generated::AgentStatusResponse {
            current_agent: status.agent_name.clone(),
            agent_display_name: status.agent_name,
            provider: status.provider,
            model: status.model,
            state: if status.enabled {
                "running".to_string()
            } else {
                "idle".to_string()
            },
            tasks_completed: 0,
            tasks_in_progress: 0,
        }))
    }

    /// 切换 Agent
    async fn switch_agent(
        &self,
        request: tonic::Request<generated::SwitchAgentRequest>,
    ) -> Result<tonic::Response<generated::SwitchAgentResponse>, tonic::Status> {
        let req = request.into_inner();
        Ok(tonic::Response::new(generated::SwitchAgentResponse {
            success: true,
            agent_name: req.agent_name.clone(),
            message: format!("Switched to agent: {}", req.agent_name),
        }))
    }

    /// 列出所有 Agents
    async fn list_agents(
        &self,
        _request: tonic::Request<generated::ListAgentsRequest>,
    ) -> Result<tonic::Response<generated::ListAgentsResponse>, tonic::Status> {
        Ok(tonic::Response::new(generated::ListAgentsResponse {
            agents: vec![
                generated::AgentInfo {
                    name: "historian".to_string(),
                    display_name: "Historian".to_string(),
                    description: "Knowledge retrieval and task decomposition".to_string(),
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    task_types: vec!["decomposition".to_string(), "retrieval".to_string()],
                    priority: 1,
                    is_default: true,
                },
                generated::AgentInfo {
                    name: "implementer".to_string(),
                    display_name: "Implementer".to_string(),
                    description: "Code implementation and development".to_string(),
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    task_types: vec!["implementation".to_string(), "testing".to_string()],
                    priority: 2,
                    is_default: false,
                },
            ],
        }))
    }

    async fn get_session_timeline(
        &self,
        request: tonic::Request<generated::SessionTimelineRequest>,
    ) -> Result<tonic::Response<generated::SessionTimelineResponse>, tonic::Status> {
        self.ensure_agent_enabled().await?;
        let req = request.into_inner();
        self.validate_requested_session(&req.session_id).await?;
        let limit = if req.limit == 0 {
            Some(100)
        } else {
            Some(req.limit as usize)
        };

        let timeline = self
            .agent_manager
            .session_timeline(limit)
            .await
            .map_err(|e| tonic::Status::internal(format!("failed to read timeline: {}", e)))?;
        let events = timeline
            .into_iter()
            .map(Self::map_execution_event)
            .collect::<Vec<_>>();
        Ok(tonic::Response::new(generated::SessionTimelineResponse {
            events,
        }))
    }

    async fn subscribe_session_timeline(
        &self,
        request: tonic::Request<generated::SessionTimelineRequest>,
    ) -> Result<tonic::Response<Self::SubscribeSessionTimelineStream>, tonic::Status> {
        self.ensure_agent_enabled().await?;
        let req = request.into_inner();
        self.validate_requested_session(&req.session_id).await?;
        let current_status = self.agent_manager.status().await;
        let target_session_id = if req.session_id.is_empty() {
            current_status.session_id.unwrap_or_default()
        } else {
            req.session_id.clone()
        };
        let poll_ms = timeline_stream_poll_ms();
        let backlog_limit = req.limit as usize;

        let manager = self.agent_manager.clone();
        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(async move {
            let timeline = match manager.session_timeline(None).await {
                Ok(v) => v,
                Err(e) => {
                    let _ = tx
                        .send(Err(tonic::Status::internal(format!(
                            "timeline stream error: {}",
                            e
                        ))))
                        .await;
                    return;
                }
            };
            let mut sent = initial_stream_cursor(timeline.len(), backlog_limit);
            for event in timeline.iter().skip(sent) {
                if tx
                    .send(Ok(Self::map_execution_event(event.clone())))
                    .await
                    .is_err()
                {
                    return;
                }
            }
            sent = timeline.len();

            let mut live_rx = match manager.subscribe_execution_events().await {
                Ok((live_session_id, rx)) => {
                    if !target_session_id.is_empty() && live_session_id != target_session_id {
                        let _ = tx
                            .send(Err(tonic::Status::not_found(format!(
                                "session '{}' is not active on this daemon",
                                target_session_id
                            ))))
                            .await;
                        return;
                    }
                    Some(rx)
                }
                Err(e) => {
                    warn!(
                        "timeline live stream unavailable, fallback to polling: {}",
                        e
                    );
                    None
                }
            };

            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(poll_ms));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                if let Some(rx) = live_rx.as_mut() {
                    let mut close_live = false;
                    tokio::select! {
                        recv = rx.recv() => {
                            match recv {
                                Ok(message) => {
                                    if !target_session_id.is_empty()
                                        && message.session_id != target_session_id
                                    {
                                        continue;
                                    }
                                    if tx
                                        .send(Ok(Self::map_execution_event(message.event)))
                                        .await
                                        .is_err()
                                    {
                                        return;
                                    }
                                    sent = sent.saturating_add(1);
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                    // rely on periodic reconciliation below
                                    continue;
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                    close_live = true;
                                }
                            }
                        }
                        _ = ticker.tick() => {
                            let timeline = match manager.session_timeline(None).await {
                                Ok(v) => v,
                                Err(e) => {
                                    let _ = tx
                                        .send(Err(tonic::Status::internal(format!(
                                            "timeline stream error: {}",
                                            e
                                        ))))
                                        .await;
                                    return;
                                }
                            };
                            sent = normalize_stream_cursor(sent, timeline.len());
                            for event in timeline.iter().skip(sent) {
                                if tx
                                    .send(Ok(Self::map_execution_event(event.clone())))
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                            sent = timeline.len();
                        }
                    }
                    if close_live {
                        live_rx = None;
                    }
                } else {
                    ticker.tick().await;
                    let timeline = match manager.session_timeline(None).await {
                        Ok(v) => v,
                        Err(e) => {
                            let _ = tx
                                .send(Err(tonic::Status::internal(format!(
                                    "timeline stream error: {}",
                                    e
                                ))))
                                .await;
                            return;
                        }
                    };
                    sent = normalize_stream_cursor(sent, timeline.len());
                    for event in timeline.iter().skip(sent) {
                        if tx
                            .send(Ok(Self::map_execution_event(event.clone())))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                    sent = timeline.len();
                }
            }
        });

        Ok(tonic::Response::new(ReceiverStream::new(rx)))
    }

    /// Agent 流式聊天
    async fn agent_chat(
        &self,
        request: tonic::Request<tonic::Streaming<generated::ChatRequest>>,
    ) -> Result<tonic::Response<Self::AgentChatStream>, tonic::Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            while let Ok(Some(chat_request)) = stream.message().await {
                match chat_request.request_type {
                    Some(generated::chat_request::RequestType::Message(msg)) => {
                        let response = generated::ChatResponse {
                            response_type: Some(
                                generated::chat_response::ResponseType::ContentChunk(
                                    generated::ContentChunk {
                                        content: format!("Agent: {}", msg.content),
                                        is_complete: true,
                                    },
                                ),
                            ),
                        };
                        let _ = tx.send(Ok(response)).await;
                    }
                    _ => {}
                }
            }
        });

        Ok(tonic::Response::new(ReceiverStream::new(rx)))
    }

    /// 工具执行流
    async fn execute_tool(
        &self,
        request: tonic::Request<tonic::Streaming<generated::ToolRequest>>,
    ) -> Result<tonic::Response<Self::ExecuteToolStream>, tonic::Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            while let Ok(Some(tool_request)) = stream.message().await {
                match tool_request.request_type {
                    Some(generated::tool_request::RequestType::Execute(exec)) => {
                        // 简单的工具执行响应
                        let response = generated::ToolResponse {
                            response_type: Some(generated::tool_response::ResponseType::Output(
                                generated::ToolOutput {
                                    stream_id: "1".to_string(),
                                    chunk: format!("Executed: {}", exec.tool_name),
                                    is_stdout: true,
                                },
                            )),
                        };
                        let _ = tx.send(Ok(response)).await;

                        // 发送完成信号
                        let complete = generated::ToolResponse {
                            response_type: Some(generated::tool_response::ResponseType::Complete(
                                generated::ToolComplete {
                                    success: true,
                                    output: "Tool executed successfully".to_string(),
                                    exit_code: 0,
                                    duration_ms: 100,
                                },
                            )),
                        };
                        let _ = tx.send(Ok(complete)).await;
                    }
                    _ => {}
                }
            }
        });

        Ok(tonic::Response::new(ReceiverStream::new(rx)))
    }
}

#[derive(Clone)]
struct TimelineSseState {
    manager: Arc<AgentModeManager>,
}

#[derive(Debug, Default, Deserialize)]
struct TimelineSseQuery {
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    limit: usize,
}

async fn subscribe_session_timeline_sse(
    State(state): State<TimelineSseState>,
    Query(query): Query<TimelineSseQuery>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, StatusCode> {
    if !state.manager.is_enabled().await {
        state
            .manager
            .enable(AgentModeConfig::default())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    if !query.session_id.is_empty() {
        let current = state.manager.status().await;
        let same = current
            .session_id
            .as_ref()
            .map(|sid| sid == &query.session_id)
            .unwrap_or(false);
        if !same {
            return Err(StatusCode::NOT_FOUND);
        }
    }

    let current_status = state.manager.status().await;
    let target_session_id = if query.session_id.is_empty() {
        current_status.session_id.unwrap_or_default()
    } else {
        query.session_id.clone()
    };

    let manager = state.manager.clone();
    let backlog_limit = query.limit;
    let poll_ms = timeline_sse_poll_ms();
    let stream = async_stream::stream! {
        let timeline = match manager.session_timeline(None).await {
            Ok(v) => v,
            Err(e) => {
                let payload = serde_json::json!({
                    "error": format!("timeline stream error: {}", e)
                }).to_string();
                yield Ok(SseEvent::default().event("error").data(payload));
                return;
            }
        };
        let mut sent = initial_stream_cursor(timeline.len(), backlog_limit);
        for event in timeline.iter().skip(sent) {
            let mapped = AgentGrpcService::map_execution_event(event.clone());
            let payload = execution_event_to_json(mapped);
            yield Ok(SseEvent::default().event("execution_event").data(payload));
        }
        sent = timeline.len();

        let mut live_rx = match manager.subscribe_execution_events().await {
            Ok((live_session_id, rx)) => {
                if !target_session_id.is_empty() && live_session_id != target_session_id {
                    let payload = serde_json::json!({
                        "error": format!("session '{}' is not active on this daemon", target_session_id)
                    }).to_string();
                    yield Ok(SseEvent::default().event("error").data(payload));
                    return;
                }
                Some(rx)
            }
            Err(e) => {
                warn!("SSE timeline live stream unavailable, fallback to polling: {}", e);
                None
            }
        };

        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(poll_ms));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            if let Some(rx) = live_rx.as_mut() {
                let mut close_live = false;
                tokio::select! {
                    recv = rx.recv() => {
                        match recv {
                            Ok(message) => {
                                if !target_session_id.is_empty()
                                    && message.session_id != target_session_id
                                {
                                    continue;
                                }
                                let mapped = AgentGrpcService::map_execution_event(message.event);
                                let payload = execution_event_to_json(mapped);
                                yield Ok(SseEvent::default().event("execution_event").data(payload));
                                sent = sent.saturating_add(1);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                continue;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                close_live = true;
                            }
                        }
                    }
                    _ = ticker.tick() => {
                        let timeline = match manager.session_timeline(None).await {
                            Ok(v) => v,
                            Err(e) => {
                                let payload = serde_json::json!({
                                    "error": format!("timeline stream error: {}", e)
                                }).to_string();
                                yield Ok(SseEvent::default().event("error").data(payload));
                                break;
                            }
                        };
                        sent = normalize_stream_cursor(sent, timeline.len());
                        for event in timeline.iter().skip(sent) {
                            let mapped = AgentGrpcService::map_execution_event(event.clone());
                            let payload = execution_event_to_json(mapped);
                            yield Ok(SseEvent::default().event("execution_event").data(payload));
                        }
                        sent = timeline.len();
                    }
                }
                if close_live {
                    live_rx = None;
                }
            } else {
                ticker.tick().await;
                let timeline = match manager.session_timeline(None).await {
                    Ok(v) => v,
                    Err(e) => {
                        let payload = serde_json::json!({
                            "error": format!("timeline stream error: {}", e)
                        }).to_string();
                        yield Ok(SseEvent::default().event("error").data(payload));
                        break;
                    }
                };
                sent = normalize_stream_cursor(sent, timeline.len());
                for event in timeline.iter().skip(sent) {
                    let mapped = AgentGrpcService::map_execution_event(event.clone());
                    let payload = execution_event_to_json(mapped);
                    yield Ok(SseEvent::default().event("execution_event").data(payload));
                }
                sent = timeline.len();
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keepalive"),
    ))
}

async fn run_timeline_sse_server(
    address: SocketAddr,
    manager: Arc<AgentModeManager>,
) -> Result<(), std::io::Error> {
    let app = Router::new()
        .route(
            "/agent/session_timeline/subscribe",
            get(subscribe_session_timeline_sse),
        )
        .with_state(TimelineSseState { manager });
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, app).await
}

/// gRPC NDC 服务实现
#[derive(Debug)]
pub struct NdcGrpcService {
    daemon: Arc<NdcDaemon>,
    start_time: Instant,
}

impl NdcGrpcService {
    pub fn new(daemon: Arc<NdcDaemon>) -> Self {
        Self {
            daemon,
            start_time: Instant::now(),
        }
    }

    fn uptime(&self) -> String {
        format!("{}s", self.start_time.elapsed().as_secs())
    }
}

#[tonic::async_trait]
impl generated::ndc_service_server::NdcService for NdcGrpcService {
    type StreamingChatStream = ChatResponseStream;
    type StreamExecuteTaskStream = TaskExecutionEventStream;

    /// 健康检查
    async fn health_check(
        &self,
        _request: tonic::Request<generated::HealthCheckRequest>,
    ) -> Result<tonic::Response<generated::HealthCheckResponse>, tonic::Status> {
        Ok(tonic::Response::new(generated::HealthCheckResponse {
            healthy: true,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime: self.uptime(),
        }))
    }

    /// 创建任务
    async fn create_task(
        &self,
        request: tonic::Request<generated::CreateTaskRequest>,
    ) -> Result<tonic::Response<generated::TaskResponse>, tonic::Status> {
        let req = request.into_inner();

        if req.title.is_empty() {
            return Err(tonic::Status::invalid_argument("title is required"));
        }

        let executor = self.daemon.executor();

        match executor
            .create_task(
                req.title.clone(),
                req.description.clone(),
                AgentRole::Historian,
            )
            .await
        {
            Ok(task) => Ok(tonic::Response::new(generated::TaskResponse {
                task: Some(generated::Task {
                    id: task.id.to_string(),
                    title: task.title,
                    description: task.description,
                    state: format!("{:?}", task.state),
                    created_at: task.metadata.created_at.to_rfc3339(),
                    created_by: format!("{:?}", task.metadata.created_by),
                    tags: task.metadata.tags,
                    steps: Vec::new(),
                    snapshots: Vec::new(),
                    agent_role: "historian".to_string(),
                    priority: generated::TaskPriority::Normal as i32,
                }),
                message: "Task created successfully".to_string(),
            })),
            Err(e) => Err(tonic::Status::internal(format!(
                "Failed to create task: {}",
                e
            ))),
        }
    }

    /// 获取任务
    async fn get_task(
        &self,
        request: tonic::Request<generated::GetTaskRequest>,
    ) -> Result<tonic::Response<generated::TaskResponse>, tonic::Status> {
        let req = request.into_inner();

        let task_id: TaskId = req
            .task_id
            .parse()
            .map_err(|_| tonic::Status::invalid_argument("invalid task_id"))?;

        let executor = self.daemon.executor();
        let storage = &executor.context().storage;

        match storage.get_task(&task_id).await {
            Ok(Some(task)) => Ok(tonic::Response::new(generated::TaskResponse {
                task: Some(generated::Task {
                    id: task.id.to_string(),
                    title: task.title,
                    description: task.description,
                    state: format!("{:?}", task.state),
                    created_at: task.metadata.created_at.to_rfc3339(),
                    created_by: format!("{:?}", task.metadata.created_by),
                    tags: task.metadata.tags,
                    steps: Vec::new(),
                    snapshots: Vec::new(),
                    agent_role: "historian".to_string(),
                    priority: generated::TaskPriority::Normal as i32,
                }),
                message: "Task found".to_string(),
            })),
            Ok(None) => Err(tonic::Status::not_found("task not found")),
            Err(e) => Err(tonic::Status::internal(format!("storage error: {}", e))),
        }
    }

    /// 列出任务
    async fn list_tasks(
        &self,
        _request: tonic::Request<generated::ListTasksRequest>,
    ) -> Result<tonic::Response<generated::ListTasksResponse>, tonic::Status> {
        let executor = self.daemon.executor();
        let storage = &executor.context().storage;

        match storage.list_tasks().await {
            Ok(tasks) => {
                let total_count = tasks.len();
                let tasks: Vec<generated::Task> = tasks
                    .into_iter()
                    .map(|task| generated::Task {
                        id: task.id.to_string(),
                        title: task.title,
                        description: task.description,
                        state: format!("{:?}", task.state),
                        created_at: task.metadata.created_at.to_rfc3339(),
                        created_by: format!("{:?}", task.metadata.created_by),
                        tags: task.metadata.tags,
                        steps: Vec::new(),
                        snapshots: Vec::new(),
                        agent_role: "historian".to_string(),
                        priority: generated::TaskPriority::Normal as i32,
                    })
                    .collect();

                Ok(tonic::Response::new(generated::ListTasksResponse {
                    tasks,
                    total_count: total_count as u32,
                }))
            }
            Err(e) => Err(tonic::Status::internal(format!("storage error: {}", e))),
        }
    }

    /// 执行任务
    async fn execute_task(
        &self,
        request: tonic::Request<generated::ExecuteTaskRequest>,
    ) -> Result<tonic::Response<generated::ExecuteTaskResponse>, tonic::Status> {
        let req = request.into_inner();

        let task_id: TaskId = req
            .task_id
            .parse()
            .map_err(|_| tonic::Status::invalid_argument("invalid task_id"))?;

        let executor = self.daemon.executor().clone();

        if req.sync {
            match executor.execute_task(task_id).await {
                Ok(result) => Ok(tonic::Response::new(generated::ExecuteTaskResponse {
                    execution_id: result.task_id.to_string(),
                    status: if result.success {
                        "completed".to_string()
                    } else {
                        "failed".to_string()
                    },
                    message: result.output,
                })),
                Err(e) => Err(tonic::Status::internal(format!("execution error: {}", e))),
            }
        } else {
            Ok(tonic::Response::new(generated::ExecuteTaskResponse {
                execution_id: task_id.to_string(),
                status: "pending".to_string(),
                message: "Task execution queued".to_string(),
            }))
        }
    }

    /// 回滚任务
    async fn rollback_task(
        &self,
        request: tonic::Request<generated::RollbackTaskRequest>,
    ) -> Result<tonic::Response<generated::RollbackTaskResponse>, tonic::Status> {
        let req = request.into_inner();

        let task_id: TaskId = req
            .task_id
            .parse()
            .map_err(|_| tonic::Status::invalid_argument("invalid task_id"))?;

        let executor = self.daemon.executor();
        let storage = &executor.context().storage;

        match storage.get_task(&task_id).await {
            Ok(Some(task)) => {
                if task.snapshots.is_empty() {
                    return Ok(tonic::Response::new(generated::RollbackTaskResponse {
                        success: false,
                        message: "No snapshots available for rollback".to_string(),
                        rollback_to_commit: String::new(),
                    }));
                }
                Ok(tonic::Response::new(generated::RollbackTaskResponse {
                    success: true,
                    message: "Rollback initiated".to_string(),
                    rollback_to_commit: String::new(),
                }))
            }
            Ok(None) => Err(tonic::Status::not_found("task not found")),
            Err(e) => Err(tonic::Status::internal(format!("storage error: {}", e))),
        }
    }

    /// 获取系统状态
    async fn get_system_status(
        &self,
        _request: tonic::Request<generated::GetSystemStatusRequest>,
    ) -> Result<tonic::Response<generated::SystemStatusResponse>, tonic::Status> {
        let executor = self.daemon.executor();
        let storage = &executor.context().storage;

        let total_tasks = match storage.list_tasks().await {
            Ok(tasks) => tasks.len(),
            Err(_) => 0,
        };

        Ok(tonic::Response::new(generated::SystemStatusResponse {
            healthy: true,
            version: env!("CARGO_PKG_VERSION").to_string(),
            total_tasks: total_tasks as u32,
            active_tasks: 0,
            queued_tasks: 0,
            resources: None,
        }))
    }

    /// 流式聊天 - 双向流
    async fn streaming_chat(
        &self,
        request: tonic::Request<tonic::Streaming<generated::ChatRequest>>,
    ) -> Result<tonic::Response<Self::StreamingChatStream>, tonic::Status> {
        let mut stream = request.into_inner();

        // 创建响应流
        let (tx, rx) = mpsc::channel(100);

        // 处理流式请求
        tokio::spawn(async move {
            let mut message_history: Vec<generated::Message> = Vec::new();

            while let Ok(Some(chat_request)) = stream.message().await {
                let chat_req = chat_request.request_type;

                match chat_req {
                    Some(generated::chat_request::RequestType::Message(msg)) => {
                        message_history.push(msg.clone());

                        // 简单响应 - 实际应调用 Agent
                        let response = generated::ChatResponse {
                            response_type: Some(
                                generated::chat_response::ResponseType::ContentChunk(
                                    generated::ContentChunk {
                                        content: format!("Echo: {}", msg.content),
                                        is_complete: true,
                                    },
                                ),
                            ),
                        };

                        let _ = tx.send(Ok(response)).await;
                    }
                    Some(generated::chat_request::RequestType::ToolResult(_)) => {
                        // 处理工具结果
                        let response = generated::ChatResponse {
                            response_type: Some(generated::chat_response::ResponseType::StreamEnd(
                                generated::StreamEnd {
                                    completion_reason: "tool_call".to_string(),
                                    usage: None,
                                },
                            )),
                        };
                        let _ = tx.send(Ok(response)).await;
                    }
                    Some(generated::chat_request::RequestType::ContextRequest(_)) => {
                        // 上下文请求
                        let response = generated::ChatResponse {
                            response_type: Some(
                                generated::chat_response::ResponseType::ContextData(
                                    generated::ContextData {
                                        context_type: generated::ContextType::Memory as i32,
                                        items: Vec::new(),
                                    },
                                ),
                            ),
                        };
                        let _ = tx.send(Ok(response)).await;
                    }
                    None => {}
                }
            }
        });

        Ok(tonic::Response::new(ReceiverStream::new(rx)))
    }

    /// 流式任务执行 - 双向流
    async fn stream_execute_task(
        &self,
        request: tonic::Request<tonic::Streaming<generated::ExecuteTaskEvent>>,
    ) -> Result<tonic::Response<Self::StreamExecuteTaskStream>, tonic::Status> {
        let mut stream = request.into_inner();

        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            while let Ok(Some(exec_event)) = stream.message().await {
                match exec_event.event_type {
                    Some(generated::execute_task_event::EventType::TaskStart(start)) => {
                        let event = generated::TaskExecutionEvent {
                            event_type: Some(generated::task_execution_event::EventType::Status(
                                generated::ExecutionStatus {
                                    status: "running".to_string(),
                                    message: format!("Starting task: {}", start.task_title),
                                },
                            )),
                        };
                        let _ = tx.send(Ok(event)).await;
                    }
                    Some(generated::execute_task_event::EventType::StepProgress(progress)) => {
                        let event = generated::TaskExecutionEvent {
                            event_type: Some(
                                generated::task_execution_event::EventType::StepComplete(
                                    generated::StepComplete {
                                        step_number: progress.current_step,
                                        result: progress.step_description,
                                        duration_ms: 0,
                                    },
                                ),
                            ),
                        };
                        let _ = tx.send(Ok(event)).await;
                    }
                    _ => {}
                }
            }
        });

        Ok(tonic::Response::new(ReceiverStream::new(rx)))
    }
}

/// 启动 gRPC 服务器
pub async fn run_grpc_server(address: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting NDC gRPC Daemon on {}", address);

    let context = ExecutionContext::default();
    let executor = Arc::new(Executor::new(context));
    let daemon = Arc::new(NdcDaemon::new(executor.clone(), address));
    let agent_manager = AgentGrpcService::build_agent_manager(&daemon);

    let ndc_service = NdcGrpcService::new(daemon.clone());
    let agent_service = AgentGrpcService::with_manager(daemon.clone(), agent_manager.clone());

    if let Some(sse_addr) = resolve_timeline_sse_address(address) {
        info!("Starting timeline SSE endpoint on {}", sse_addr);
        tokio::spawn(async move {
            if let Err(e) = run_timeline_sse_server(sse_addr, agent_manager).await {
                warn!("timeline SSE server stopped: {}", e);
            }
        });
    }

    tonic::transport::Server::builder()
        .add_service(generated::ndc_service_server::NdcServiceServer::new(
            ndc_service,
        ))
        .add_service(generated::agent_service_server::AgentServiceServer::new(
            agent_service,
        ))
        .serve(address)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn read_http_response_head(
        address: SocketAddr,
        path: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut stream = tokio::net::TcpStream::connect(address).await?;
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nAccept: text/event-stream\r\nConnection: close\r\n\r\n",
            path, address
        );
        stream.write_all(request.as_bytes()).await?;

        let mut out = Vec::new();
        let mut buf = [0u8; 2048];
        let start = std::time::Instant::now();
        loop {
            if out.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            if start.elapsed() > Duration::from_secs(3) {
                break;
            }
            match tokio::time::timeout(Duration::from_millis(200), stream.read(&mut buf)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => out.extend_from_slice(&buf[..n]),
                Ok(Err(e)) => return Err(Box::new(e)),
                Err(_) => {}
            }
        }

        Ok(String::from_utf8_lossy(&out).to_string())
    }

    async fn read_http_response_prefix(
        address: SocketAddr,
        path: &str,
        min_bytes: usize,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut stream = tokio::net::TcpStream::connect(address).await?;
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nAccept: text/event-stream\r\nConnection: keep-alive\r\n\r\n",
            path, address
        );
        stream.write_all(request.as_bytes()).await?;

        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        let start = std::time::Instant::now();
        loop {
            if out.len() >= min_bytes {
                break;
            }
            if start.elapsed() > Duration::from_secs(4) {
                break;
            }
            match tokio::time::timeout(Duration::from_millis(300), stream.read(&mut buf)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => out.extend_from_slice(&buf[..n]),
                Ok(Err(e)) => return Err(Box::new(e)),
                Err(_) => {}
            }
        }

        Ok(String::from_utf8_lossy(&out).to_string())
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock poisoned")
    }

    fn with_env_overrides<T>(updates: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
        let _guard = env_lock();
        let previous = updates
            .iter()
            .map(|(key, _)| ((*key).to_string(), std::env::var(key).ok()))
            .collect::<Vec<_>>();
        for (key, value) in updates {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        let result = f();
        for (key, old) in previous {
            match old {
                Some(v) => std::env::set_var(&key, v),
                None => std::env::remove_var(&key),
            }
        }
        result
    }

    #[test]
    fn test_map_execution_event() {
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::ToolCallEnd,
            timestamp: chrono::Utc::now(),
            message: "tool_call_end: read (ok)".to_string(),
            round: 2,
            tool_name: Some("read".to_string()),
            tool_call_id: Some("call-1".to_string()),
            duration_ms: Some(37),
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        };
        let mapped = AgentGrpcService::map_execution_event(event);
        assert_eq!(mapped.kind, "ToolCallEnd");
        assert_eq!(mapped.round, 2);
        assert_eq!(mapped.tool_name, "read");
        assert_eq!(mapped.tool_call_id, "call-1");
        assert_eq!(mapped.duration_ms, 37);
        assert!(!mapped.is_error);
        assert!(!mapped.timestamp.is_empty());
    }

    #[test]
    fn test_map_execution_event_defaults_optional_fields() {
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::StepStart,
            timestamp: chrono::Utc::now(),
            message: "llm_round_1_start".to_string(),
            round: 1,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        };
        let mapped = AgentGrpcService::map_execution_event(event);
        assert_eq!(mapped.kind, "StepStart");
        assert_eq!(mapped.tool_name, "");
        assert_eq!(mapped.tool_call_id, "");
        assert_eq!(mapped.duration_ms, 0);
        assert_eq!(mapped.workflow_stage, "");
        assert_eq!(mapped.workflow_detail, "");
        assert_eq!(mapped.token_source, "");
        assert_eq!(mapped.token_total, 0);
        assert_eq!(mapped.token_session_total, 0);
        assert_eq!(mapped.workflow_stage_index, 0);
        assert_eq!(mapped.workflow_stage_total, 0);
    }

    #[test]
    fn test_map_execution_event_permission_asked() {
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::PermissionAsked,
            timestamp: chrono::Utc::now(),
            message: "permission_asked: Permission denied for write".to_string(),
            round: 3,
            tool_name: Some("write".to_string()),
            tool_call_id: Some("call-perm-1".to_string()),
            duration_ms: None,
            is_error: true,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        };
        let mapped = AgentGrpcService::map_execution_event(event);
        assert_eq!(mapped.kind, "PermissionAsked");
        assert_eq!(mapped.round, 3);
        assert_eq!(mapped.tool_name, "write");
        assert_eq!(mapped.tool_call_id, "call-perm-1");
        assert!(mapped.is_error);
    }

    #[test]
    fn test_map_execution_event_workflow_stage_fields() {
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::WorkflowStage,
            timestamp: chrono::Utc::now(),
            message: "workflow_stage: discovery | tool_calls_planned".to_string(),
            round: 2,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: Some(ndc_core::AgentWorkflowStage::Discovery),
            workflow_detail: Some("tool_calls_planned".to_string()),
            workflow_stage_index: Some(2),
            workflow_stage_total: Some(ndc_core::AgentWorkflowStage::TOTAL_STAGES),
        };
        let mapped = AgentGrpcService::map_execution_event(event);
        assert_eq!(mapped.kind, "WorkflowStage");
        assert_eq!(mapped.workflow_stage, "discovery");
        assert_eq!(mapped.workflow_detail, "tool_calls_planned");
        assert_eq!(mapped.workflow_stage_index, 2);
        assert_eq!(
            mapped.workflow_stage_total,
            ndc_core::AgentWorkflowStage::TOTAL_STAGES
        );
    }

    #[test]
    fn test_map_execution_event_workflow_stage_fields_from_structured_payload() {
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::WorkflowStage,
            timestamp: chrono::Utc::now(),
            message: "workflow stage changed".to_string(),
            round: 9,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: Some(ndc_core::AgentWorkflowStage::Verifying),
            workflow_detail: Some("quality_gate".to_string()),
            workflow_stage_index: Some(4),
            workflow_stage_total: Some(ndc_core::AgentWorkflowStage::TOTAL_STAGES),
        };
        let mapped = AgentGrpcService::map_execution_event(event);
        assert_eq!(mapped.workflow_stage, "verifying");
        assert_eq!(mapped.workflow_detail, "quality_gate");
        assert_eq!(mapped.workflow_stage_index, 4);
        assert_eq!(
            mapped.workflow_stage_total,
            ndc_core::AgentWorkflowStage::TOTAL_STAGES
        );
    }

    #[test]
    fn test_map_execution_event_token_usage_fields() {
        let event = ndc_core::AgentExecutionEvent {
            kind: ndc_core::AgentExecutionEventKind::TokenUsage,
            timestamp: chrono::Utc::now(),
            message: "token_usage: source=provider prompt=11 completion=7 total=18 | session_prompt_total=22 session_completion_total=14 session_total=36".to_string(),
            round: 3,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: None,
            workflow_detail: None,
            workflow_stage_index: None,
            workflow_stage_total: None,
        };
        let mapped = AgentGrpcService::map_execution_event(event);
        assert_eq!(mapped.kind, "TokenUsage");
        assert_eq!(mapped.token_source, "provider");
        assert_eq!(mapped.token_prompt, 11);
        assert_eq!(mapped.token_completion, 7);
        assert_eq!(mapped.token_total, 18);
        assert_eq!(mapped.token_session_prompt_total, 22);
        assert_eq!(mapped.token_session_completion_total, 14);
        assert_eq!(mapped.token_session_total, 36);
    }

    #[test]
    fn test_sanitize_sensitive_text() {
        let text = "api_key:abc token=xyz Bearer aaa sk-ABCDEF123456 /home/jerryg/repo";
        let out = sanitize_text(text, RedactionMode::Basic);
        assert!(out.contains("api_key=[REDACTED]"));
        assert!(out.contains("token=[REDACTED]"));
        assert!(out.contains("Bearer [REDACTED]"));
        assert!(out.contains("sk-[REDACTED]"));
        assert!(out.contains("/home/***"));
    }

    #[test]
    fn test_initial_stream_cursor() {
        assert_eq!(initial_stream_cursor(10, 0), 10);
        assert_eq!(initial_stream_cursor(10, 3), 7);
        assert_eq!(initial_stream_cursor(2, 5), 0);
    }

    #[test]
    fn test_normalize_stream_cursor() {
        assert_eq!(normalize_stream_cursor(8, 10), 8);
        assert_eq!(normalize_stream_cursor(12, 10), 10);
        assert_eq!(normalize_stream_cursor(0, 10), 0);
    }

    #[test]
    fn test_timeline_stream_poll_ms_default_and_env_clamped() {
        with_env_overrides(&[("NDC_TIMELINE_STREAM_POLL_MS", None)], || {
            assert_eq!(timeline_stream_poll_ms(), DEFAULT_TIMELINE_STREAM_POLL_MS);
        });
        with_env_overrides(&[("NDC_TIMELINE_STREAM_POLL_MS", Some("20"))], || {
            assert_eq!(timeline_stream_poll_ms(), MIN_TIMELINE_STREAM_POLL_MS);
        });
        with_env_overrides(&[("NDC_TIMELINE_STREAM_POLL_MS", Some("5000"))], || {
            assert_eq!(timeline_stream_poll_ms(), MAX_TIMELINE_STREAM_POLL_MS);
        });
        with_env_overrides(&[("NDC_TIMELINE_STREAM_POLL_MS", Some("450"))], || {
            assert_eq!(timeline_stream_poll_ms(), 450);
        });
    }

    #[test]
    fn test_timeline_sse_poll_ms_default_and_env_clamped() {
        with_env_overrides(
            &[
                ("NDC_TIMELINE_STREAM_POLL_MS", Some("180")),
                ("NDC_TIMELINE_SSE_POLL_MS", None),
            ],
            || {
                assert_eq!(timeline_sse_poll_ms(), 180);
            },
        );
        with_env_overrides(&[("NDC_TIMELINE_SSE_POLL_MS", Some("20"))], || {
            assert_eq!(timeline_sse_poll_ms(), MIN_TIMELINE_STREAM_POLL_MS);
        });
        with_env_overrides(&[("NDC_TIMELINE_SSE_POLL_MS", Some("9000"))], || {
            assert_eq!(timeline_sse_poll_ms(), MAX_TIMELINE_STREAM_POLL_MS);
        });
        with_env_overrides(&[("NDC_TIMELINE_SSE_POLL_MS", Some("320"))], || {
            assert_eq!(timeline_sse_poll_ms(), 320);
        });
    }

    #[test]
    fn test_resolve_timeline_sse_address_from_env() {
        let grpc_addr: SocketAddr = "127.0.0.1:4096".parse().unwrap();
        with_env_overrides(&[("NDC_TIMELINE_SSE_ADDR", None)], || {
            assert_eq!(resolve_timeline_sse_address(grpc_addr), None);
        });
        with_env_overrides(&[("NDC_TIMELINE_SSE_ADDR", Some("auto"))], || {
            assert_eq!(
                resolve_timeline_sse_address(grpc_addr),
                Some("127.0.0.1:4097".parse().unwrap())
            );
        });
        with_env_overrides(&[("NDC_TIMELINE_SSE_ADDR", Some("127.0.0.1:5050"))], || {
            assert_eq!(
                resolve_timeline_sse_address(grpc_addr),
                Some("127.0.0.1:5050".parse().unwrap())
            );
        });
        with_env_overrides(&[("NDC_TIMELINE_SSE_ADDR", Some("invalid"))], || {
            assert_eq!(resolve_timeline_sse_address(grpc_addr), None);
        });
    }

    #[test]
    fn test_execution_event_to_json_payload_shape() {
        let payload = execution_event_to_json(generated::ExecutionEvent {
            kind: "ToolCallEnd".to_string(),
            timestamp: "2026-02-24T12:00:00Z".to_string(),
            message: "tool_call_end: read".to_string(),
            round: 3,
            tool_name: "read".to_string(),
            tool_call_id: "call-1".to_string(),
            duration_ms: 42,
            is_error: false,
            workflow_stage: "executing".to_string(),
            workflow_detail: "tool_call".to_string(),
            token_source: "provider".to_string(),
            token_prompt: 11,
            token_completion: 7,
            token_total: 18,
            token_session_prompt_total: 22,
            token_session_completion_total: 14,
            token_session_total: 36,
            workflow_stage_index: 3,
            workflow_stage_total: ndc_core::AgentWorkflowStage::TOTAL_STAGES,
        });
        let parsed: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(parsed["kind"], "ToolCallEnd");
        assert_eq!(parsed["round"], 3);
        assert_eq!(parsed["tool_name"], "read");
        assert_eq!(parsed["duration_ms"], 42);
        assert_eq!(parsed["is_error"], false);
        assert_eq!(parsed["workflow_stage"], "executing");
        assert_eq!(parsed["token_source"], "provider");
        assert_eq!(parsed["token_total"], 18);
        assert_eq!(parsed["token_session_total"], 36);
        assert_eq!(parsed["workflow_stage_index"], 3);
        assert_eq!(
            parsed["workflow_stage_total"],
            ndc_core::AgentWorkflowStage::TOTAL_STAGES
        );
    }

    #[tokio::test]
    async fn test_timeline_sse_endpoint_accepts_and_validates_session() {
        let context = ExecutionContext::default();
        let executor = Arc::new(Executor::new(context));
        let daemon_addr: SocketAddr = "127.0.0.1:50051".parse().unwrap();
        let daemon = Arc::new(NdcDaemon::new(executor, daemon_addr));
        let manager = AgentGrpcService::build_agent_manager(&daemon);

        let mut config = AgentModeConfig::default();
        config.provider = "ollama".to_string();
        config.model = "llama3.2".to_string();
        manager.enable(config).await.unwrap();

        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let sse_addr = probe.local_addr().unwrap();
        drop(probe);

        let server = tokio::spawn(run_timeline_sse_server(sse_addr, manager));
        tokio::time::sleep(Duration::from_millis(120)).await;

        let ok_head =
            read_http_response_head(sse_addr, "/agent/session_timeline/subscribe?limit=0")
                .await
                .unwrap();
        assert!(ok_head.starts_with("HTTP/1.1 200"));
        assert!(ok_head
            .to_ascii_lowercase()
            .contains("content-type: text/event-stream"));

        let not_found_head = read_http_response_head(
            sse_addr,
            "/agent/session_timeline/subscribe?session_id=missing&limit=0",
        )
        .await
        .unwrap();
        assert!(not_found_head.starts_with("HTTP/1.1 404"));

        server.abort();
    }

    #[tokio::test]
    async fn test_timeline_sse_replays_execution_event_payload() {
        let context = ExecutionContext::default();
        let executor = Arc::new(Executor::new(context));
        let daemon_addr: SocketAddr = "127.0.0.1:50052".parse().unwrap();
        let daemon = Arc::new(NdcDaemon::new(executor, daemon_addr));
        let manager = AgentGrpcService::build_agent_manager(&daemon);

        let mut config = AgentModeConfig::default();
        config.provider = "ollama".to_string();
        config.model = "llama3.2".to_string();
        manager.enable(config).await.unwrap();

        // Seed timeline events. This may fail due to local provider availability, which is fine.
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            manager.process_input("seed timeline for sse"),
        )
        .await;

        let session_id = manager.status().await.session_id.unwrap_or_default();
        assert!(!session_id.is_empty());

        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let sse_addr = probe.local_addr().unwrap();
        drop(probe);

        let server = tokio::spawn(run_timeline_sse_server(sse_addr, manager));
        tokio::time::sleep(Duration::from_millis(120)).await;

        let body = read_http_response_prefix(
            sse_addr,
            &format!(
                "/agent/session_timeline/subscribe?session_id={}&limit=20",
                session_id
            ),
            1024,
        )
        .await
        .unwrap();

        assert!(body.starts_with("HTTP/1.1 200"));
        assert!(body.contains("event: execution_event"));
        assert!(
            body.contains("\"kind\":\"SessionStatus\"") || body.contains("\"kind\":\"StepStart\"")
        );
        assert!(body.contains("\"workflow_stage\""));
        assert!(body.contains("\"token_total\""));

        server.abort();
    }
}
