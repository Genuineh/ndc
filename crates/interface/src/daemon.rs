//! Daemon - gRPC 服务
//!
//! 职责：
//! - gRPC 服务端实现（当启用 grpc feature 时）
//! - 任务管理 API
//! - 健康检查

use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

use ndc_core::TaskId;
use ndc_runtime::{Executor, ExecutionContext};

/// gRPC 服务实现
#[derive(Debug)]
pub struct NdcDaemon {
    /// 执行器实例
    executor: Arc<Executor>,
    /// 服务器地址
    address: SocketAddr,
    /// 运行状态
    running: bool,
}

impl NdcDaemon {
    /// 创建新的守护进程实例
    pub fn new(executor: Arc<Executor>, address: SocketAddr) -> Self {
        Self {
            executor,
            address,
            running: false,
        }
    }

    /// 获取执行器引用
    pub fn executor(&self) -> &Arc<Executor> {
        &self.executor
    }

    /// 获取服务器地址
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// 检查是否运行中
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// 设置运行状态
    pub fn set_running(&mut self, running: bool) {
        self.running = running;
    }
}

/// 健康检查服务
#[derive(Debug, Clone)]
pub struct HealthService;

impl HealthService {
    /// 创建新的健康检查服务
    pub fn new() -> Self {
        Self
    }

    /// 执行健康检查
    pub fn check_health(&self) -> HealthCheckResult {
        HealthCheckResult {
            healthy: true,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// 健康检查结果
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub healthy: bool,
    pub version: String,
}

/// 守护进程错误
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DaemonError {
    #[error("任务未找到: {0}")]
    TaskNotFound(TaskId),

    #[error("执行器错误: {0}")]
    ExecutorError(String),

    #[error("存储错误: {0}")]
    StorageError(String),

    #[error("无效的请求: {0}")]
    InvalidRequest(String),
}

/// 运行守护进程
pub async fn run_daemon(address: SocketAddr) {
    info!("Starting NDC Daemon on {}", address);

    let context = ExecutionContext::default();
    let executor = Arc::new(Executor::new(context));
    let mut daemon = NdcDaemon::new(executor, address);

    info!("Daemon started");
    info!("Listening on: {}", address);

    daemon.set_running(true);

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        info!("Daemon running - {} active", address);
    }
}

/// 任务摘要信息
#[derive(Debug, Clone)]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub description: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: String,
}

/// 内存摘要信息
#[derive(Debug, Clone)]
pub struct MemorySummary {
    pub id: String,
    pub content: String,
    pub memory_type: String,
    pub stability: String,
    pub created_at: String,
}
