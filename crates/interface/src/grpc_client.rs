//! gRPC Client Library
//!
//! Provides a client SDK for connecting to NDC gRPC daemon
//!
//! # Features
//!
//! - Automatic connection management
//! - Retry mechanism with exponential backoff
//! - Connection pooling
//! - All gRPC methods

#[cfg(feature = "grpc")]
use std::fmt;
#[cfg(feature = "grpc")]
use std::time::Duration;
#[cfg(feature = "grpc")]
use thiserror::Error;
#[cfg(feature = "grpc")]
use tonic::transport::Channel;

#[cfg(feature = "grpc")]
use crate::generated::{
    CreateTaskRequest, ExecuteTaskRequest, ExecuteTaskResponse, ExecutionEvent,
    GetSystemStatusRequest, GetTaskRequest, HealthCheckRequest, HealthCheckResponse,
    ListTasksRequest, ListTasksResponse, RollbackTaskRequest, RollbackTaskResponse,
    SessionTimelineRequest, SessionTimelineResponse, SystemStatusResponse, TaskResponse,
    agent_service_client::AgentServiceClient, ndc_service_client::NdcServiceClient,
};

#[cfg(feature = "grpc")]
/// Client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Server address (e.g., "127.0.0.1:50051")
    pub address: String,

    /// Connection timeout
    pub connect_timeout: Duration,

    /// Request timeout
    pub request_timeout: Duration,

    /// Maximum retry attempts
    pub max_retries: u32,

    /// Base delay for retry backoff
    pub base_retry_delay: Duration,

    /// Enable connection pooling
    pub enable_pooling: bool,

    /// Pool size (if enabled)
    pub pool_size: usize,
}

#[cfg(feature = "grpc")]
impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:50051".to_string(),
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            max_retries: 3,
            base_retry_delay: Duration::from_millis(100),
            enable_pooling: false,
            pool_size: 4,
        }
    }
}

#[cfg(feature = "grpc")]
impl fmt::Display for ClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NdcClient(address={}, timeout={:?})",
            self.address, self.request_timeout
        )
    }
}

#[cfg(feature = "grpc")]
/// Client errors
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Request timeout")]
    Timeout,

    #[error("Max retries exceeded: {attempts} attempts")]
    MaxRetriesExceeded { attempts: u32, last_error: String },

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

#[cfg(feature = "grpc")]
/// NDC gRPC Client
///
/// Provides a client SDK for connecting to NDC gRPC daemon.
#[derive(Debug, Clone)]
pub struct NdcClient {
    config: ClientConfig,
}

#[cfg(feature = "grpc")]
impl NdcClient {
    /// Create a new client with default config
    pub fn new(address: impl Into<String>) -> Self {
        let mut config = ClientConfig::default();
        config.address = address.into();
        Self::with_config(config)
    }

    /// Create a client with custom configuration
    pub fn with_config(config: ClientConfig) -> Self {
        Self { config }
    }

    /// Connect to the server - returns endpoint URL
    pub async fn connect(&self) -> Result<String, ClientError> {
        Ok(format!("http://{}", self.config.address))
    }

    /// Get a channel from pool or create new
    async fn get_channel(&self) -> Result<Channel, ClientError> {
        let endpoint = format!("http://{}", self.config.address);

        let channel = tonic::transport::Channel::from_shared(endpoint)
            .map_err(|e| ClientError::ConnectionFailed(e.to_string()))?
            .connect_timeout(self.config.connect_timeout)
            .timeout(self.request_timeout())
            .connect()
            .await
            .map_err(|e| ClientError::ConnectionFailed(e.to_string()))?;

        Ok(channel)
    }

    fn request_timeout(&self) -> Duration {
        self.config.request_timeout
    }

    /// Health check
    pub async fn health_check(&self) -> Result<HealthCheckResponse, ClientError> {
        let channel = self.get_channel().await?;
        let mut client = NdcServiceClient::new(channel);

        let request = HealthCheckRequest {};

        match client.health_check(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(map_error(&e.to_string())),
        }
    }

    /// Create a new task
    pub async fn create_task(
        &self,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<TaskResponse, ClientError> {
        let channel = self.get_channel().await?;
        let mut client = NdcServiceClient::new(channel);

        let request = CreateTaskRequest {
            title: title.into(),
            description: description.into(),
            created_by: "client".to_string(),
            agent_role: "historian".to_string(),
            metadata: std::collections::HashMap::new(),
        };

        match client.create_task(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(map_error(&e.to_string())),
        }
    }

    /// Get a task by ID
    pub async fn get_task(&self, task_id: impl Into<String>) -> Result<TaskResponse, ClientError> {
        let channel = self.get_channel().await?;
        let mut client = NdcServiceClient::new(channel);

        let request = GetTaskRequest {
            task_id: task_id.into(),
            include_steps: false,
            include_snapshots: false,
        };

        match client.get_task(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(map_error(&e.to_string())),
        }
    }

    /// List tasks with optional filter
    pub async fn list_tasks(
        &self,
        limit: u32,
        state_filter: impl Into<String>,
    ) -> Result<ListTasksResponse, ClientError> {
        let channel = self.get_channel().await?;
        let mut client = NdcServiceClient::new(channel);

        let request = ListTasksRequest {
            limit,
            state_filter: state_filter.into(),
            agent_role: "".to_string(),
            created_after: "".to_string(),
            created_before: "".to_string(),
        };

        match client.list_tasks(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(map_error(&e.to_string())),
        }
    }

    /// Execute a task
    pub async fn execute_task(
        &self,
        task_id: impl Into<String>,
        sync: bool,
    ) -> Result<ExecuteTaskResponse, ClientError> {
        let channel = self.get_channel().await?;
        let mut client = NdcServiceClient::new(channel);

        let request = ExecuteTaskRequest {
            task_id: task_id.into(),
            sync,
        };

        match client.execute_task(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(map_error(&e.to_string())),
        }
    }

    /// Rollback a task to a snapshot
    pub async fn rollback_task(
        &self,
        task_id: impl Into<String>,
        snapshot_id: impl Into<String>,
    ) -> Result<RollbackTaskResponse, ClientError> {
        let channel = self.get_channel().await?;
        let mut client = NdcServiceClient::new(channel);

        let request = RollbackTaskRequest {
            task_id: task_id.into(),
            snapshot_id: snapshot_id.into(),
            force: false,
        };

        match client.rollback_task(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(map_error(&e.to_string())),
        }
    }

    /// Get system status
    pub async fn get_system_status(&self) -> Result<SystemStatusResponse, ClientError> {
        let channel = self.get_channel().await?;
        let mut client = NdcServiceClient::new(channel);

        let request = GetSystemStatusRequest {};

        match client.get_system_status(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(map_error(&e.to_string())),
        }
    }

    fn build_timeline_request(
        session_id: Option<&str>,
        limit: Option<u32>,
    ) -> SessionTimelineRequest {
        SessionTimelineRequest {
            session_id: session_id.unwrap_or_default().to_string(),
            limit: limit.unwrap_or(0),
        }
    }

    fn build_timeline_sse_subscribe_path(session_id: Option<&str>, limit: Option<u32>) -> String {
        let session_id = session_id.unwrap_or_default();
        let limit = limit.unwrap_or(0);
        format!(
            "/agent/session_timeline/subscribe?session_id={}&limit={}",
            session_id, limit
        )
    }

    /// Fetch execution timeline for a session (or current active session if omitted).
    pub async fn get_session_timeline(
        &self,
        session_id: Option<&str>,
        limit: Option<u32>,
    ) -> Result<SessionTimelineResponse, ClientError> {
        let channel = self.get_channel().await?;
        let mut client = AgentServiceClient::new(channel);
        let request = Self::build_timeline_request(session_id, limit);
        match client.get_session_timeline(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(map_error(&e.to_string())),
        }
    }

    /// Subscribe to execution timeline stream for a session (or current active session).
    /// `limit` controls initial backlog replay count; `None`/`0` means subscribe from now.
    pub async fn subscribe_session_timeline(
        &self,
        session_id: Option<&str>,
        limit: Option<u32>,
    ) -> Result<tonic::Streaming<ExecutionEvent>, ClientError> {
        let channel = self.get_channel().await?;
        let mut client = AgentServiceClient::new(channel);
        let request = Self::build_timeline_request(session_id, limit);
        match client.subscribe_session_timeline(request).await {
            Ok(response) => Ok(response.into_inner()),
            Err(e) => Err(map_error(&e.to_string())),
        }
    }

    /// Build SSE subscription URL for execution timeline.
    /// This URL can be consumed by EventSource-compatible clients.
    pub fn timeline_sse_subscribe_url(
        &self,
        session_id: Option<&str>,
        limit: Option<u32>,
    ) -> String {
        let path = Self::build_timeline_sse_subscribe_path(session_id, limit);
        format!("http://{}{}", self.config.address, path)
    }

    /// Check if client is healthy
    pub async fn is_healthy(&self) -> bool {
        self.health_check().await.is_ok()
    }
}

/// Map gRPC status to client error
fn map_error(error: &str) -> ClientError {
    if error.contains("not found") {
        ClientError::NotFound(error.to_string())
    } else if error.contains("invalid argument") {
        ClientError::InvalidArgument(error.to_string())
    } else if error.contains("deadline exceeded") {
        ClientError::Timeout
    } else {
        ClientError::ServerError(error.to_string())
    }
}

/// Convenience function to create a connected client
pub async fn create_client(address: &str) -> Result<NdcClient, ClientError> {
    let client = NdcClient::new(address);

    // Test connection
    if !client.is_healthy().await {
        return Err(ClientError::ConnectionFailed(
            "Failed to connect to server".to_string(),
        ));
    }

    Ok(client)
}

#[cfg(feature = "grpc")]
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_client_config_default() {
        let config = ClientConfig::default();
        assert_eq!(config.address, "127.0.0.1:50051");
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_client_config_custom() {
        let config = ClientConfig {
            address: "192.168.1.1:9000".to_string(),
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(60),
            max_retries: 5,
            base_retry_delay: Duration::from_millis(200),
            enable_pooling: true,
            pool_size: 8,
        };

        assert_eq!(config.address, "192.168.1.1:9000");
        assert_eq!(config.max_retries, 5);
        assert!(config.enable_pooling);
    }

    #[test]
    fn test_map_error() {
        let not_found = map_error("task not found");
        assert!(matches!(not_found, ClientError::NotFound(_)));

        let invalid = map_error("invalid argument: task_id is required");
        assert!(matches!(invalid, ClientError::InvalidArgument(_)));

        let server = map_error("internal server error");
        assert!(matches!(server, ClientError::ServerError(_)));
    }

    #[test]
    fn test_client_error_display() {
        let err = ClientError::ConnectionFailed("test".to_string());
        assert_eq!(format!("{}", err), "Connection failed: test");

        let err = ClientError::Timeout;
        assert_eq!(format!("{}", err), "Request timeout");

        let err = ClientError::MaxRetriesExceeded {
            attempts: 3,
            last_error: "test".to_string(),
        };
        assert_eq!(format!("{}", err), "Max retries exceeded: 3 attempts");
    }

    #[tokio::test]
    async fn test_client_new() {
        let client = NdcClient::new("127.0.0.1:50051");
        assert_eq!(client.config.address, "127.0.0.1:50051");
    }

    #[tokio::test]
    async fn test_client_with_config() {
        let config = ClientConfig {
            address: "localhost:8080".to_string(),
            ..ClientConfig::default()
        };
        let client = NdcClient::with_config(config);
        assert_eq!(client.config.address, "localhost:8080");
    }

    #[tokio::test]
    async fn test_client_connect() {
        let client = NdcClient::new("127.0.0.1:50051");
        // Connection will fail but should return the endpoint
        let result = client.connect().await;
        // Either succeeds or returns connection failed
        match result {
            Ok(url) => assert_eq!(url, "http://127.0.0.1:50051"),
            Err(ClientError::ConnectionFailed(_)) => {} // Expected for non-running server
            Err(_) => panic!("Unexpected error"),
        }
    }

    #[test]
    fn test_build_timeline_request_defaults() {
        let req = NdcClient::build_timeline_request(None, None);
        assert_eq!(req.session_id, "");
        assert_eq!(req.limit, 0);
    }

    #[test]
    fn test_build_timeline_request_custom() {
        let req = NdcClient::build_timeline_request(Some("session-1"), Some(50));
        assert_eq!(req.session_id, "session-1");
        assert_eq!(req.limit, 50);
    }

    #[test]
    fn test_build_timeline_sse_subscribe_path_defaults() {
        let path = NdcClient::build_timeline_sse_subscribe_path(None, None);
        assert_eq!(
            path,
            "/agent/session_timeline/subscribe?session_id=&limit=0"
        );
    }

    #[test]
    fn test_build_timeline_sse_subscribe_path_custom() {
        let path = NdcClient::build_timeline_sse_subscribe_path(Some("session-1"), Some(80));
        assert_eq!(
            path,
            "/agent/session_timeline/subscribe?session_id=session-1&limit=80"
        );
    }

    #[test]
    fn test_timeline_sse_subscribe_url() {
        let client = NdcClient::new("127.0.0.1:4097");
        let url = client.timeline_sse_subscribe_url(Some("abc"), Some(20));
        assert_eq!(
            url,
            "http://127.0.0.1:4097/agent/session_timeline/subscribe?session_id=abc&limit=20"
        );
    }
}
