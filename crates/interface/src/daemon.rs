//! Daemon - gRPC 服务
//!
//! 职责：
//! - gRPC 服务端实现
//! - 任务管理 API
//! - 健康检查

use std::net::SocketAddr;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, warn, error};

use ndc_runtime::Executor;

use crate::ndc_interface;

/// gRPC 服务实现
#[derive(Debug)]
pub struct NdcDaemon {
    /// 执行器
    executor: std::sync::Arc<ndc_runtime::Executor>,
}

impl NdcDaemon {
    /// 创建新的 Daemon 实例
    pub fn new(executor: std::sync::Arc<ndc_runtime::Executor>) -> Self {
        Self {
            executor,
        }
    }
}

/// 健康检查服务
#[derive(Debug)]
pub struct HealthService;

#[tonic::derive_service]
impl HealthService {
    async fn check(&self, _request: Request<()>) -> Result<Response<()>, Status> {
        Ok(Response::new(()))
    }
}

/// 任务服务
#[derive(Debug)]
pub struct TaskService {
    daemon: NdcDaemon,
}

impl TaskService {
    pub fn new(daemon: NdcDaemon) -> Self {
        Self { daemon }
    }
}

#[tonic::derive_service]
impl TaskService {
    async fn create_task(
        &self,
        request: Request<ndc_interface::CreateTaskRequest>,
    ) -> Result<Response<ndc_interface::TaskResponse>, Status> {
        let req = request.into_inner();

        info!("gRPC: Creating task: {}", req.title);

        // 使用 Task::new 方法创建任务
        let task = ndc_core::Task::new(
            req.title,
            req.description,
            ndc_core::AgentRole::Historian,
        );

        Ok(Response::new(ndc_interface::TaskResponse {
            task: Some(task),
        }))
    }

    async fn get_task(
        &self,
        request: Request<ndc_interface::GetTaskRequest>,
    ) -> Result<Response<ndc_interface::TaskResponse>, Status> {
        let req = request.into_inner();

        info!("gRPC: Getting task: {}", req.task_id);

        // TODO: 实现任务获取
        Err(Status::not_found("Task not found"))
    }

    async fn list_tasks(
        &self,
        request: Request<ndc_interface::ListTasksRequest>,
    ) -> Result<Response<ndc_interface::ListTasksResponse>, Status> {
        let _req = request.into_inner();

        info!("gRPC: Listing tasks");

        // TODO: 实现任务列表
        Ok(Response::new(ndc_interface::ListTasksResponse {
            tasks: vec![],
            total_count: 0,
        }))
    }

    async fn execute_task(
        &self,
        request: Request<ndc_interface::ExecuteTaskRequest>,
    ) -> Result<Response<ndc_interface::ExecuteTaskResponse>, Status> {
        let req = request.into_inner();

        info!("gRPC: Executing task: {}", req.task_id);

        // TODO: 实现任务执行
        Ok(Response::new(ndc_interface::ExecuteTaskResponse {
            execution_id: "".to_string(),
            status: ndc_interface::ExecutionStatus::Pending as i32,
        }))
    }
}

/// 运行 Daemon
pub async fn run_daemon(address: SocketAddr) {
    info!("Starting NDC Daemon on {}", address);

    // 初始化组件
    let executor = std::sync::Arc::new(ndc_runtime::Executor::default());

    // 存储和记忆层初始化（可选）
    warn!("Storage and memory features not available in basic mode");

    let daemon = NdcDaemon::new(executor);

    // 创建 gRPC 服务
    let health_service = HealthService {};
    let task_service = TaskService::new(daemon);

    // 构建服务
    let mut builder = tonic::transport::Server::builder();

    // 添加健康检查服务
    builder.add_service(tonic::health::check_service());

    // TODO: 添加任务服务（需要定义 proto 文件）
    // let task_router = builder.add_service(TaskServiceServer::new(task_service));

    info!("NDC Daemon started on {}", address);

    // 阻塞运行
    if let Err(e) = builder.serve(address).await {
        error!("Daemon error: {}", e);
    }
}

/// 生成 proto 文件（用于开发者参考）
pub fn generate_proto_files() {
    println!(r#"
syntax = "proto3";

package ndc;

service NdcService {
    rpc HealthCheck(HealthCheckRequest) returns (HealthCheckResponse);

    rpc CreateTask(CreateTaskRequest) returns (TaskResponse);
    rpc GetTask(GetTaskRequest) returns (TaskResponse);
    rpc ListTasks(ListTasksRequest) returns (ListTasksResponse);
    rpc ExecuteTask(ExecuteTaskRequest) returns (ExecuteTaskResponse);
    rpc RollbackTask(RollbackTaskRequest) returns (RollbackTaskResponse);

    rpc SearchMemory(SearchMemoryRequest) returns (SearchMemoryResponse);
    rpc StoreMemory(StoreMemoryRequest) returns (StoreMemoryResponse);
}

message HealthCheckRequest {}
message HealthCheckResponse { bool healthy = 1; }

message CreateTaskRequest {
    string title = 1;
    string description = 2;
    string task_type = 3;
    string created_by = 4;
}

message GetTaskRequest { string task_id = 1; }

message ListTasksRequest {
    optional string state_filter = 1;
    uint32 limit = 2;
    uint32 offset = 3;
}

message TaskResponse { optional Task task = 1; }

message ListTasksResponse {
    repeated Task tasks = 1;
    uint32 total_count = 2;
}

message ExecuteTaskRequest {
    string task_id = 1;
    bool sync = 2;
}

message ExecuteTaskResponse {
    string execution_id = 1;
    ExecutionStatus status = 2;
    string result = 3;
}

enum ExecutionStatus {
    PENDING = 0;
    RUNNING = 1;
    COMPLETED = 2;
    FAILED = 3;
}

message RollbackTaskRequest {
    string task_id = 1;
    optional string snapshot_id = 2;
}

message RollbackTaskResponse {
    bool success = 1;
    string message = 2;
}

message SearchMemoryRequest {
    string query = 1;
    optional string stability_filter = 2;
    uint32 limit = 3;
}

message SearchMemoryResponse {
    repeated Memory memories = 1;
}

message Memory {
    string id = 1;
    string content = 2;
    string memory_type = 3;
    Stability stability = 4;
}

enum Stability {
    EPHEMERAL = 0;
    DERIVED = 1;
    VERIFIED = 2;
    CANONICAL = 3;
}

message StoreMemoryRequest {
    string content = 1;
    string memory_type = 2;
    Stability stability = 3;
}

message StoreMemoryResponse { bool success = 1; }

message Task {
    string id = 1;
    string title = 2;
    string description = 3;
    string task_type = 4;
    TaskState state = 5;
    string created_at = 6;
    string updated_at = 7;
    string created_by = 8;
    Priority priority = 9;
    optional Intent intent = 10;
    optional Snapshot snapshot = 11;
}

enum TaskState {
    PENDING = 0;
    PREPARING = 1;
    IN_PROGRESS = 2;
    AWAITING_VERIFICATION = 3;
    COMPLETED = 4;
    FAILED = 5;
    ROLLED_BACK = 6;
}

enum Priority {
    LOW = 0;
    MEDIUM = 1;
    HIGH = 2;
    CRITICAL = 3;
}
"#);
}
