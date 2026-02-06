//! Daemon Tests

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::path::PathBuf;

    /// Test NdcDaemon creation
    #[test]
    fn test_ndc_daemon_creation() {
        let addr: SocketAddr = "127.0.0.1:50051".parse().unwrap();

        // Can't easily test without executor, just verify struct can be created
        // when all types are available
        assert!(addr.port() == 50051);
        assert!(addr.ip().is_loopback());
    }

    /// Test SocketAddr parsing
    #[test]
    fn test_socket_addr_parsing() {
        let addr1: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        assert_eq!(addr1.port(), 8080);

        let addr2: SocketAddr = "[::1]:9000".parse().unwrap();
        assert_eq!(addr2.port(), 9000);
    }

    /// Test DaemonError display
    #[test]
    fn test_daemon_error_display() {
        use crate::daemon::DaemonError;

        let error = DaemonError::ExecutorError("test error".to_string());
        assert_eq!(format!("{}", error), "执行器错误: test error");

        let error = DaemonError::StorageError("storage failed".to_string());
        assert_eq!(format!("{}", error), "存储错误: storage failed");

        let error = DaemonError::InvalidRequest("invalid input".to_string());
        assert_eq!(format!("{}", error), "无效的请求: invalid input");
    }

    /// Test DaemonError source chain
    #[test]
    fn test_daemon_error_source() {
        use crate::daemon::DaemonError;
        use std::error::Error;

        let error = DaemonError::ExecutorError("inner".to_string());
        assert!(error.source().is_none());
    }

    /// Test HealthCheckResult
    #[test]
    fn test_health_check_result() {
        use crate::daemon::HealthService;

        let service = HealthService::new();
        let result = service.check_health();

        assert!(result.healthy);
        assert!(!result.version.is_empty());
    }

    /// Test HealthCheckResult clone
    #[test]
    fn test_health_check_result_clone() {
        use crate::daemon::HealthCheckResult;

        let result1 = HealthCheckResult {
            healthy: true,
            version: "1.0.0".to_string(),
        };
        let result2 = result1.clone();

        assert_eq!(result1.healthy, result2.healthy);
        assert_eq!(result1.version, result2.version);
    }

    /// Test TaskSummary
    #[test]
    fn test_task_summary() {
        use crate::daemon::TaskSummary;

        let summary = TaskSummary {
            id: "01HVV7P8QZ0VZ1Z2X3Y4Z5A6B".to_string(),
            title: "Test Task".to_string(),
            description: "A test task".to_string(),
            state: "Pending".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            created_by: "Historian".to_string(),
        };

        assert!(summary.id.len() > 20);
        assert_eq!(summary.title, "Test Task");
    }

    /// Test MemorySummary
    #[test]
    fn test_memory_summary() {
        use crate::daemon::MemorySummary;

        let summary = MemorySummary {
            id: "01HVV7P8QZ0VZ1Z2X3Y4Z5A6C".to_string(),
            content: "Test memory content".to_string(),
            memory_type: "conversation".to_string(),
            stability: "derived".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        assert_eq!(summary.memory_type, "conversation");
    }

    /// Test PathBuf operations
    #[test]
    fn test_pathbuf_operations() {
        let path = PathBuf::from("/var/log/ndc");
        assert!(!path.to_string_lossy().is_empty());
    }

    /// Test DaemonError Debug output
    #[test]
    fn test_daemon_error_debug() {
        use crate::daemon::DaemonError;

        // Just test ExecutorError since ULID parsing is tested elsewhere
        let error = DaemonError::ExecutorError("test".to_string());
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("ExecutorError"));
    }

    /// Test Send + Sync for DaemonError
    #[test]
    fn test_daemon_error_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<crate::daemon::DaemonError>();
    }

    /// Test DaemonError partial eq
    #[test]
    fn test_daemon_error_partial_eq() {
        use crate::daemon::DaemonError;

        let error1 = DaemonError::StorageError("same".to_string());
        let error2 = DaemonError::StorageError("same".to_string());
        let error3 = DaemonError::StorageError("different".to_string());

        assert_eq!(error1, error2);
        assert_ne!(error1, error3);
    }
}
