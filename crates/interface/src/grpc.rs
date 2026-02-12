//! gRPC 服务实现（当启用 grpc feature 时）
//!
//! 使用 tonic 框架提供 gRPC 服务

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;
use tokio_stream::wrappers::ReceiverStream;
use tokio::sync::mpsc;

use ndc_core::TaskId;
use ndc_core::AgentRole;
use ndc_runtime::{Executor, ExecutionContext};

use crate::daemon::NdcDaemon;

// Re-export generated types from the proto
pub use super::generated;

// Type aliases for streaming responses
type ChatResponseStream = ReceiverStream<Result<generated::ChatResponse, tonic::Status>>;
type TaskExecutionEventStream = ReceiverStream<Result<generated::TaskExecutionEvent, tonic::Status>>;
type ToolResponseStream = ReceiverStream<Result<generated::ToolResponse, tonic::Status>>;

/// gRPC Agent 服务实现
#[derive(Debug)]
pub struct AgentGrpcService {
    daemon: Arc<NdcDaemon>,
}

impl AgentGrpcService {
    pub fn new(daemon: Arc<NdcDaemon>) -> Self {
        Self { daemon }
    }
}

#[tonic::async_trait]
impl generated::agent_service_server::AgentService for AgentGrpcService {
    type AgentChatStream = ChatResponseStream;
    type ExecuteToolStream = ToolResponseStream;

    /// 获取 Agent 状态
    async fn get_agent_status(
        &self,
        _request: tonic::Request<generated::AgentStatusRequest>,
    ) -> Result<tonic::Response<generated::AgentStatusResponse>, tonic::Status> {
        Ok(tonic::Response::new(generated::AgentStatusResponse {
            current_agent: "historian".to_string(),
            agent_display_name: "Historian".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            state: "idle".to_string(),
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
                            response_type: Some(generated::chat_response::ResponseType::ContentChunk(
                                generated::ContentChunk {
                                    content: format!("Agent: {}", msg.content),
                                    is_complete: true,
                                }
                            )),
                        };
                        let _ = tx.send(Ok(response)).await;
                    }
                    _ => {}
                }
            }
        });

        Ok(tonic::Response::new(
            ReceiverStream::new(rx)
        ))
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
                                }
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
                                }
                            )),
                        };
                        let _ = tx.send(Ok(complete)).await;
                    }
                    _ => {}
                }
            }
        });

        Ok(tonic::Response::new(
            ReceiverStream::new(rx)
        ))
    }
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

        match executor.create_task(
            req.title.clone(),
            req.description.clone(),
            AgentRole::Historian,
        ).await {
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
            Err(e) => Err(tonic::Status::internal(format!("Failed to create task: {}", e))),
        }
    }

    /// 获取任务
    async fn get_task(
        &self,
        request: tonic::Request<generated::GetTaskRequest>,
    ) -> Result<tonic::Response<generated::TaskResponse>, tonic::Status> {
        let req = request.into_inner();

        let task_id: TaskId = req.task_id.parse()
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
                let tasks: Vec<generated::Task> = tasks.into_iter().map(|task| generated::Task {
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
                }).collect();

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

        let task_id: TaskId = req.task_id.parse()
            .map_err(|_| tonic::Status::invalid_argument("invalid task_id"))?;

        let executor = self.daemon.executor().clone();

        if req.sync {
            match executor.execute_task(task_id).await {
                Ok(result) => Ok(tonic::Response::new(generated::ExecuteTaskResponse {
                    execution_id: result.task_id.to_string(),
                    status: if result.success { "completed".to_string() } else { "failed".to_string() },
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

        let task_id: TaskId = req.task_id.parse()
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
        let executor = self.daemon.executor().clone();

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
                            response_type: Some(generated::chat_response::ResponseType::ContentChunk(
                                generated::ContentChunk {
                                    content: format!("Echo: {}", msg.content),
                                    is_complete: true,
                                }
                            )),
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
                                }
                            )),
                        };
                        let _ = tx.send(Ok(response)).await;
                    }
                    Some(generated::chat_request::RequestType::ContextRequest(_)) => {
                        // 上下文请求
                        let response = generated::ChatResponse {
                            response_type: Some(generated::chat_response::ResponseType::ContextData(
                                generated::ContextData {
                                    context_type: generated::ContextType::Memory as i32,
                                    items: Vec::new(),
                                }
                            )),
                        };
                        let _ = tx.send(Ok(response)).await;
                    }
                    None => {}
                }
            }
        });

        Ok(tonic::Response::new(
            ReceiverStream::new(rx)
        ))
    }

    /// 流式任务执行 - 双向流
    async fn stream_execute_task(
        &self,
        request: tonic::Request<tonic::Streaming<generated::ExecuteTaskEvent>>,
    ) -> Result<tonic::Response<Self::StreamExecuteTaskStream>, tonic::Status> {
        let mut stream = request.into_inner();
        let executor = self.daemon.executor().clone();

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
                                }
                            )),
                        };
                        let _ = tx.send(Ok(event)).await;
                    }
                    Some(generated::execute_task_event::EventType::StepProgress(progress)) => {
                        let event = generated::TaskExecutionEvent {
                            event_type: Some(generated::task_execution_event::EventType::StepComplete(
                                generated::StepComplete {
                                    step_number: progress.current_step,
                                    result: progress.step_description,
                                    duration_ms: 0,
                                }
                            )),
                        };
                        let _ = tx.send(Ok(event)).await;
                    }
                    _ => {}
                }
            }
        });

        Ok(tonic::Response::new(
            ReceiverStream::new(rx)
        ))
    }
}

/// 启动 gRPC 服务器
pub async fn run_grpc_server(address: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting NDC gRPC Daemon on {}", address);

    let context = ExecutionContext::default();
    let executor = Arc::new(Executor::new(context));
    let daemon = Arc::new(NdcDaemon::new(executor.clone(), address));

    let ndc_service = NdcGrpcService::new(daemon.clone());
    let agent_service = AgentGrpcService::new(daemon);

    tonic::transport::Server::builder()
        .add_service(generated::ndc_service_server::NdcServiceServer::new(ndc_service))
        .add_service(generated::agent_service_server::AgentServiceServer::new(agent_service))
        .serve(address)
        .await?;

    Ok(())
}
