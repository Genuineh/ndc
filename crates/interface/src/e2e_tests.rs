//! E2E Tests - End-to-end CLI tests
//!
//! Tests complete CLI command flows from user input to expected output.

use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use ndc_runtime::{Executor, ExecutionContext, MemoryStorage, WorkflowEngine, ToolManager, QualityGateRunner, SharedStorage, Tool};
use ndc_core::{AgentRole, TaskState};

/// Create a test executor with in-memory storage
fn create_test_executor() -> Arc<Executor> {
    let context = ExecutionContext {
        storage: Arc::new(MemoryStorage::new()),
        workflow_engine: Arc::new(WorkflowEngine::new()),
        tools: Arc::new(ToolManager::new()),
        quality_runner: Arc::new(QualityGateRunner::new()),
        project_root: std::env::current_dir().unwrap_or(PathBuf::from(".")),
        current_role: AgentRole::Historian,
    };
    Arc::new(Executor::new(context))
}

/// Create a test CLI config
fn create_test_config() -> crate::cli::CliConfig {
    crate::cli::CliConfig {
        project_root: std::env::current_dir().unwrap_or(PathBuf::from(".")),
        storage_path: PathBuf::from(".ndc/test_storage"),
        verbose: true,
        output_format: crate::cli::OutputFormat::Pretty,
    }
}

#[cfg(test)]
mod e2e_tests {
    use super::*;

    /// Test complete task lifecycle: create -> execute -> complete
    #[tokio::test]
    async fn test_task_lifecycle() {
        let executor = create_test_executor();

        // Create a task
        let task = executor.create_task(
            "E2E Test Task".to_string(),
            "Created by E2E test".to_string(),
            AgentRole::Historian,
        ).await.unwrap();

        assert_eq!(task.title, "E2E Test Task");
        assert_eq!(task.state, TaskState::Pending);

        // List tasks
        let tasks = executor.context().storage.list_tasks().await.unwrap();
        assert!(tasks.iter().any(|t| t.title == "E2E Test Task"));

        // Get task status
        let retrieved = executor.context().storage.get_task(&task.id).await.unwrap();
        assert!(retrieved.is_some());
        let task_ref = retrieved.unwrap();
        assert_eq!(task_ref.id, task.id);
        assert_eq!(task_ref.state, TaskState::Pending);
    }

    /// Test task execution workflow
    #[tokio::test]
    async fn test_task_workflow_transitions() {
        let executor = create_test_executor();

        // Create task
        let task = executor.create_task(
            "Workflow Test".to_string(),
            "Test state transitions".to_string(),
            AgentRole::Historian,
        ).await.unwrap();

        assert_eq!(task.state, TaskState::Pending);

        // Full execution
        let result = executor.execute_task(task.id).await.unwrap();
        assert!(result.success);
        assert_eq!(result.final_state, TaskState::Completed);
    }

    /// Test parallel task creation
    #[tokio::test]
    async fn test_parallel_task_creation() {
        let executor = create_test_executor();

        let tasks: Vec<_> = (0..5).map(|i| {
            executor.create_task(
                format!("Parallel Task {}", i),
                format!("Created in parallel {}", i),
                AgentRole::Historian,
            )
        }).collect();

        // Wait for all tasks
        let results: Vec<_> = futures::future::join_all(tasks).await;
        assert_eq!(results.len(), 5);

        // All should succeed
        for result in results {
            assert!(result.is_ok());
        }

        // Verify all created
        let all_tasks = executor.context().storage.list_tasks().await.unwrap();
        assert_eq!(all_tasks.len(), 5);
    }

    /// Test task with steps execution
    #[tokio::test]
    async fn test_task_with_file_operations() {
        let executor = create_test_executor();
        let temp_dir = TempDir::new().unwrap();

        // Create a task that reads a file
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, World!").unwrap();

        // Create task
        let task = executor.create_task(
            "Read File Task".to_string(),
            "Task that reads a file".to_string(),
            AgentRole::Historian,
        ).await.unwrap();

        // Get task and manually add a step
        let mut task = executor.context().storage.get_task(&task.id).await.unwrap().unwrap();

        // Add a read file step
        let step = ndc_core::ExecutionStep {
            step_id: 1,
            action: ndc_core::Action::ReadFile {
                path: file_path.clone(),
            },
            status: ndc_core::StepStatus::Completed,
            result: Some(ndc_core::ActionResult {
                success: true,
                output: "Read completed".to_string(),
                error: None,
                metrics: ndc_core::ActionMetrics::default(),
            }),
            executed_at: Some(chrono::Utc::now()),
        };

        task.steps.push(step);
        executor.context().storage.save_task(&task).await.unwrap();

        // Verify
        let retrieved = executor.context().storage.get_task(&task.id).await.unwrap().unwrap();
        assert_eq!(retrieved.steps.len(), 1);
    }

    /// Test storage operations
    #[tokio::test]
    async fn test_storage_operations() {
        let executor = create_test_executor();

        // Create task
        let task = executor.create_task(
            "Storage Test".to_string(),
            "Test storage operations".to_string(),
            AgentRole::Historian,
        ).await.unwrap();

        // List all tasks
        let all_tasks = executor.context().storage.list_tasks().await.unwrap();
        assert!(all_tasks.iter().any(|t| t.id == task.id));
    }

    /// Test workflow engine transitions
    #[tokio::test]
    async fn test_workflow_transitions() {
        let executor = create_test_executor();
        let workflow_engine = &executor.context().workflow_engine;

        // Get available transitions for Pending state
        let can_prepare = workflow_engine.can_transition(&TaskState::Pending, &TaskState::Preparing);
        assert!(can_prepare);

        // Get transitions for InProgress state
        let can_verify = workflow_engine.can_transition(&TaskState::InProgress, &TaskState::AwaitingVerification);
        assert!(can_verify);

        // Pending cannot go directly to InProgress
        let can_direct = workflow_engine.can_transition(&TaskState::Pending, &TaskState::InProgress);
        assert!(!can_direct);

        // Completed can go back to Pending (for retry)
        let can_retry = workflow_engine.can_transition(&TaskState::Completed, &TaskState::Pending);
        assert!(can_retry);

        // Failed can go back to Pending (for retry)
        let can_retry_failed = workflow_engine.can_transition(&TaskState::Failed, &TaskState::Pending);
        assert!(can_retry_failed);
    }

    /// Test tool manager integration
    #[tokio::test]
    async fn test_tool_manager_lookup() {
        let executor = create_test_executor();
        let tool_manager = &executor.context().tools;

        // Default ToolManager doesn't have FsTool registered
        // But we can create FsTool directly
        let fs_tool = ndc_runtime::tools::FsTool::new();
        assert_eq!(fs_tool.name(), "fs");
        assert_eq!(fs_tool.description(), "File system operations: read, write, create, delete, list");

        // Non-existent tool
        let nonexistent = tool_manager.get("nonexistent");
        assert!(nonexistent.is_none());
    }

    /// Test quality gate runner
    #[tokio::test]
    async fn test_quality_gate_runner_exists() {
        let executor = create_test_executor();
        let quality_runner = &executor.context().quality_runner;

        // Quality gate runner should exist
        assert!(!format!("{:?}", quality_runner).is_empty());
    }

    /// Test task ID uniqueness
    #[tokio::test]
    async fn test_task_id_uniqueness() {
        let executor = create_test_executor();

        // Create multiple tasks
        let ids: Vec<_> = (0..10).map(|_| {
            executor.create_task(
                format!("Unique ID Test"),
                "Test".to_string(),
                AgentRole::Historian,
            )
        }).collect();

        let results = futures::future::join_all(ids).await;

        // Extract IDs
        let task_ids: Vec<_> = results.iter().filter_map(|r| r.as_ref().ok().map(|t| t.id)).collect();

        // All IDs should be unique
        let unique_ids: std::collections::HashSet<_> = task_ids.iter().collect();
        assert_eq!(task_ids.len(), unique_ids.len());
    }

    /// Test task with metadata
    #[tokio::test]
    async fn test_task_metadata() {
        let executor = create_test_executor();

        // Create task with specific role
        let task = executor.create_task(
            "Metadata Test".to_string(),
            "Task with metadata".to_string(),
            AgentRole::Implementer,
        ).await.unwrap();

        // Verify metadata
        assert_eq!(task.metadata.created_by, AgentRole::Implementer);
        assert_eq!(task.metadata.priority, ndc_core::TaskPriority::Medium);
        assert!(task.metadata.tags.is_empty());
    }

    /// Test execution context
    #[tokio::test]
    async fn test_execution_context() {
        let executor = create_test_executor();
        let context = executor.context();

        // Context should have all components
        assert!(!context.project_root.to_string_lossy().is_empty());
        assert!(!format!("{:?}", context.workflow_engine).is_empty());
        assert!(!format!("{:?}", context.tools).is_empty());
        assert!(!format!("{:?}", context.quality_runner).is_empty());
    }

    /// Test execution result
    #[tokio::test]
    async fn test_execution_result_structure() {
        let executor = create_test_executor();

        // Create and execute task
        let task = executor.create_task(
            "Result Structure Test".to_string(),
            "Test result structure".to_string(),
            AgentRole::Historian,
        ).await.unwrap();

        let result = executor.execute_task(task.id).await.unwrap();

        // Verify result structure
        assert!(result.success);
        assert_eq!(result.task_id, task.id);
        assert_eq!(result.final_state, TaskState::Completed);
        assert!(!result.output.is_empty());
        assert!(result.error.is_none());
        assert!(result.metrics.total_duration_ms >= 0);
    }
}

/// Integration tests for CLI with storage
#[cfg(test)]
mod cli_storage_tests {
    use super::*;
    use ndc_runtime::tools::FsTool;

    /// Test CLI config with storage path
    #[test]
    fn test_cli_config_with_storage() {
        let temp_dir = TempDir::new().unwrap();
        let config = crate::cli::CliConfig {
            storage_path: temp_dir.path().to_path_buf(),
            ..create_test_config()
        };

        assert_eq!(config.storage_path, temp_dir.path());
    }

    /// Test CLI config default values
    #[test]
    fn test_cli_config_defaults() {
        let config = create_test_config();
        assert!(config.verbose);
        assert!(config.output_format == crate::cli::OutputFormat::Pretty);
    }

    /// Test task creation from different roles
    #[tokio::test]
    async fn test_task_creation_by_role() {
        let executor = create_test_executor();

        let roles = vec![
            AgentRole::Planner,
            AgentRole::Implementer,
            AgentRole::Reviewer,
            AgentRole::Tester,
            AgentRole::Historian,
        ];

        for (i, role) in roles.iter().enumerate() {
            let task = executor.create_task(
                format!("Task by {:?}", role),
                format!("Created by role {}", i),
                *role,
            ).await.unwrap();

            assert_eq!(task.metadata.created_by, *role);
        }
    }

    /// Test state machine validity
    #[tokio::test]
    async fn test_state_machine() {
        let executor = create_test_executor();
        let workflow = &executor.context().workflow_engine;

        // Valid transitions
        assert!(workflow.can_transition(&TaskState::Pending, &TaskState::Preparing));
        assert!(workflow.can_transition(&TaskState::Preparing, &TaskState::InProgress));
        assert!(workflow.can_transition(&TaskState::InProgress, &TaskState::AwaitingVerification));
        assert!(workflow.can_transition(&TaskState::AwaitingVerification, &TaskState::Completed));

        // Invalid transitions
        assert!(!workflow.can_transition(&TaskState::Pending, &TaskState::Completed));
        assert!(!workflow.can_transition(&TaskState::Pending, &TaskState::InProgress));
        // Completed CAN go back to Pending for retry
        assert!(workflow.can_transition(&TaskState::Completed, &TaskState::Pending));
    }

    /// Test tool execution context
    #[tokio::test]
    async fn test_tool_execution() {
        // Create FsTool directly
        let fs_tool = FsTool::new();

        // Execute read operation
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "test content").unwrap();

        let params = serde_json::json!({
            "operation": "read",
            "path": test_file.to_string_lossy()
        });

        let result = fs_tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("test content"));
    }
}
