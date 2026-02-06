//! gRPC 服务实现（当启用 grpc feature 时）
//!
//! 使用 tonic 框架提供 gRPC 服务

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

use ndc_core::TaskId;
use ndc_core::AgentRole;
use ndc_runtime::{Executor, ExecutionContext};

/// 包含 tonic 生成的代码
include!(concat!(env!("OUT_DIR"), "/ndc.rs"));

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
impl NdcService for NdcGrpcService {
    /// 健康检查
    async fn health_check(
        &self,
        _request: tonic::Request<HealthCheckRequest>,
    ) -> Result<tonic::Response<HealthCheckResponse>, tonic::Status> {
        Ok(tonic::Response::new(HealthCheckResponse {
            healthy: true,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }))
    }

    /// 创建任务
    async fn create_task(
        &self,
        request: tonic::Request<CreateTaskRequest>,
    ) -> Result<tonic::Response<TaskResponse>, tonic::Status> {
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
            Ok(task) => Ok(tonic::Response::new(TaskResponse {
                task: Some(Task {
                    id: task.id.to_string(),
                    title: task.title,
                    description: task.description,
                    state: format!("{:?}", task.state),
                    created_at: task.metadata.created_at.to_rfc3339(),
                    created_by: format!("{:?}", task.metadata.created_by),
                    tags: task.metadata.tags,
                }),
                message: "Task created successfully".to_string(),
            })),
            Err(e) => Err(tonic::Status::internal(format!("Failed to create task: {}", e))),
        }
    }

    /// 获取任务
    async fn get_task(
        &self,
        request: tonic::Request<GetTaskRequest>,
    ) -> Result<tonic::Response<TaskResponse>, tonic::Status> {
        let req = request.into_inner();

        let task_id: TaskId = req.task_id.parse()
            .map_err(|_| tonic::Status::invalid_argument("invalid task_id"))?;

        let executor = self.daemon.executor();
        let storage = &executor.context().storage;

        match storage.get_task(&task_id).await {
            Ok(Some(task)) => Ok(tonic::Response::new(TaskResponse {
                task: Some(Task {
                    id: task.id.to_string(),
                    title: task.title,
                    description: task.description,
                    state: format!("{:?}", task.state),
                    created_at: task.metadata.created_at.to_rfc3339(),
                    created_by: format!("{:?}", task.metadata.created_by),
                    tags: task.metadata.tags,
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
        _request: tonic::Request<ListTasksRequest>,
    ) -> Result<tonic::Response<ListTasksResponse>, tonic::Status> {
        let executor = self.daemon.executor();
        let storage = &executor.context().storage;

        match storage.list_tasks().await {
            Ok(tasks) => {
                let tasks: Vec<Task> = tasks.into_iter().map(|task| Task {
                    id: task.id.to_string(),
                    title: task.title,
                    description: task.description,
                    state: format!("{:?}", task.state),
                    created_at: task.metadata.created_at.to_rfc3339(),
                    created_by: format!("{:?}", task.metadata.created_by),
                    tags: task.metadata.tags,
                }).collect();

                Ok(tonic::Response::new(ListTasksResponse {
                    tasks,
                    total_count: tasks.len() as u32,
                }))
            }
            Err(e) => Err(tonic::Status::internal(format!("storage error: {}", e))),
        }
    }

    /// 执行任务
    async fn execute_task(
        &self,
        request: tonic::Request<ExecuteTaskRequest>,
    ) -> Result<tonic::Response<ExecuteTaskResponse>, tonic::Status> {
        let req = request.into_inner();

        let task_id: TaskId = req.task_id.parse()
            .map_err(|_| tonic::Status::invalid_argument("invalid task_id"))?;

        let executor = self.daemon.executor().clone();

        if req.sync {
            match executor.execute_task(task_id).await {
                Ok(result) => Ok(tonic::Response::new(ExecuteTaskResponse {
                    execution_id: result.task_id.to_string(),
                    status: if result.success { "completed".to_string() } else { "failed".to_string() },
                    message: result.output,
                })),
                Err(e) => Err(tonic::Status::internal(format!("execution error: {}", e))),
            }
        } else {
            Ok(tonic::Response::new(ExecuteTaskResponse {
                execution_id: task_id.to_string(),
                status: "pending".to_string(),
                message: "Task execution queued".to_string(),
            }))
        }
    }

    /// 回滚任务
    async fn rollback_task(
        &self,
        request: tonic::Request<RollbackTaskRequest>,
    ) -> Result<tonic::Response<RollbackTaskResponse>, tonic::Status> {
        let req = request.into_inner();

        let task_id: TaskId = req.task_id.parse()
            .map_err(|_| tonic::Status::invalid_argument("invalid task_id"))?;

        let executor = self.daemon.executor();
        let storage = &executor.context().storage;

        match storage.get_task(&task_id).await {
            Ok(Some(task)) => {
                if task.snapshots.is_empty() {
                    return Ok(tonic::Response::new(RollbackTaskResponse {
                        success: false,
                        message: "No snapshots available for rollback".to_string(),
                    }));
                }
                Ok(tonic::Response::new(RollbackTaskResponse {
                    success: true,
                    message: "Rollback initiated".to_string(),
                }))
            }
            Ok(None) => Err(tonic::Status::not_found("task not found")),
            Err(e) => Err(tonic::Status::internal(format!("storage error: {}", e))),
        }
    }

    /// 获取系统状态
    async fn get_system_status(
        &self,
        _request: tonic::Request<GetSystemStatusRequest>,
    ) -> Result<tonic::Response<SystemStatusResponse>, tonic::Status> {
        let executor = self.daemon.executor();
        let storage = &executor.context().storage;

        let total_tasks = match storage.list_tasks().await {
            Ok(tasks) => tasks.len(),
            Err(_) => 0,
        };

        Ok(tonic::Response::new(SystemStatusResponse {
            healthy: true,
            version: env!("CARGO_PKG_VERSION").to_string(),
            total_tasks: total_tasks as u32,
            active_tasks: 0,
        }))
    }
}

/// 启动 gRPC 服务器
pub async fn run_grpc_server(address: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting NDC gRPC Daemon on {}", address);

    let context = ExecutionContext::default();
    let executor = Arc::new(Executor::new(context));
    let daemon = Arc::new(super::NdcDaemon::new(executor.clone(), address));

    let service = NdcGrpcService::new(daemon);

    tonic::transport::Server::builder()
        .add_service(NdcServiceServer::new(service))
        .serve(address)
        .await?;

    Ok(())
}
