//! E2E Tests - End-to-end CLI tests
//!
//! Tests complete CLI command flows from user input to expected output.

use ndc_core::{
    AccessControl, AgentId, AgentRole, Intent, IntentId, InvariantPriority, MemoryContent,
    MemoryEntry, MemoryMetadata, MemoryStability, SystemFactInput, TaskState,
};
use ndc_runtime::{
    create_default_tool_manager_with_storage, create_memory_storage, ExecutionContext, Executor,
    MemoryStorage, QualityGateRunner, Tool, ToolManager, WorkflowEngine,
};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

static DISCOVERY_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
        let task = executor
            .create_task(
                "E2E Test Task".to_string(),
                "Created by E2E test".to_string(),
                AgentRole::Historian,
            )
            .await
            .unwrap();

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
        let task = executor
            .create_task(
                "Workflow Test".to_string(),
                "Test state transitions".to_string(),
                AgentRole::Historian,
            )
            .await
            .unwrap();

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

        let tasks: Vec<_> = (0..5)
            .map(|i| {
                executor.create_task(
                    format!("Parallel Task {}", i),
                    format!("Created in parallel {}", i),
                    AgentRole::Historian,
                )
            })
            .collect();

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
        let task = executor
            .create_task(
                "Read File Task".to_string(),
                "Task that reads a file".to_string(),
                AgentRole::Historian,
            )
            .await
            .unwrap();

        // Get task and manually add a step
        let mut task = executor
            .context()
            .storage
            .get_task(&task.id)
            .await
            .unwrap()
            .unwrap();

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
        let retrieved = executor
            .context()
            .storage
            .get_task(&task.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.steps.len(), 1);
    }

    /// Test storage operations
    #[tokio::test]
    async fn test_storage_operations() {
        let executor = create_test_executor();

        // Create task
        let task = executor
            .create_task(
                "Storage Test".to_string(),
                "Test storage operations".to_string(),
                AgentRole::Historian,
            )
            .await
            .unwrap();

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
        let can_prepare =
            workflow_engine.can_transition(&TaskState::Pending, &TaskState::Preparing);
        assert!(can_prepare);

        // Get transitions for InProgress state
        let can_verify = workflow_engine
            .can_transition(&TaskState::InProgress, &TaskState::AwaitingVerification);
        assert!(can_verify);

        // Pending cannot go directly to InProgress
        let can_direct =
            workflow_engine.can_transition(&TaskState::Pending, &TaskState::InProgress);
        assert!(!can_direct);

        // Completed can go back to Pending (for retry)
        let can_retry = workflow_engine.can_transition(&TaskState::Completed, &TaskState::Pending);
        assert!(can_retry);

        // Failed can go back to Pending (for retry)
        let can_retry_failed =
            workflow_engine.can_transition(&TaskState::Failed, &TaskState::Pending);
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
        assert_eq!(
            fs_tool.description(),
            "File system operations: read, write, create, delete, list"
        );

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

    /// P0 smoke: ndc_task_create -> ndc_task_update -> ndc_task_verify
    #[tokio::test]
    async fn test_smoke_ndc_task_tools_chain() {
        let storage = create_memory_storage();
        let manager = create_default_tool_manager_with_storage(storage.clone());

        let create = manager
            .execute(
                "ndc_task_create",
                &serde_json::json!({
                    "title": "Smoke Task",
                    "description": "P0 smoke chain"
                }),
            )
            .await
            .unwrap();
        assert!(create.success);

        let id_line = create
            .output
            .lines()
            .find(|line| line.starts_with("Task ID: "))
            .expect("Task ID line not found");
        let task_id = id_line.trim_start_matches("Task ID: ").trim().to_string();

        for state in [
            "preparing",
            "in_progress",
            "awaiting_verification",
            "completed",
        ] {
            let update = manager
                .execute(
                    "ndc_task_update",
                    &serde_json::json!({
                        "task_id": task_id,
                        "state": state
                    }),
                )
                .await
                .unwrap();
            assert!(
                update.success,
                "state transition failed for {state}: {:?}",
                update.error
            );
        }

        let verify = manager
            .execute(
                "ndc_task_verify",
                &serde_json::json!({
                    "task_id": task_id
                }),
            )
            .await
            .unwrap();
        assert!(verify.success);
        assert!(verify.output.contains("PASSED"));
    }

    /// P0.5 smoke: discovery failure can block execution when configured
    #[tokio::test]
    async fn test_discovery_failure_block_mode() {
        let _guard = DISCOVERY_ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("NDC_DISCOVERY_FAILURE_MODE", "block"); }

        let mut context = ExecutionContext::default();
        let temp_dir = TempDir::new().unwrap();
        context.project_root = temp_dir.path().to_path_buf();
        let executor = Arc::new(Executor::new(context));
        let existing_file = temp_dir.path().join("main.rs");
        std::fs::write(&existing_file, "fn main() {}").unwrap();

        let task = executor
            .create_task(
                "Discovery strict mode".to_string(),
                "Should fail if discovery fails".to_string(),
                AgentRole::Historian,
            )
            .await
            .unwrap();

        let mut stored = executor
            .context()
            .storage
            .get_task(&task.id)
            .await
            .unwrap()
            .unwrap();
        stored.intent = Some(Intent {
            id: IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Implementer,
            proposed_action: ndc_core::Action::ReadFile {
                path: existing_file,
            },
            effects: Vec::new(),
            reasoning: "trigger discovery".to_string(),
            task_id: Some(task.id),
            timestamp: chrono::Utc::now(),
        });
        executor.context().storage.save_task(&stored).await.unwrap();

        let result = executor.execute_task(task.id).await;
        assert!(result.is_err());
        assert!(matches!(
            result.err().unwrap(),
            ndc_runtime::ExecutionError::DiscoveryFailed(_)
        ));

        unsafe { std::env::remove_var("NDC_DISCOVERY_FAILURE_MODE"); }
    }

    /// Discovery signals should be persisted into GoldMemory even when degrading on failure.
    #[tokio::test]
    async fn test_discovery_signal_persisted_to_gold_memory() {
        let _guard = DISCOVERY_ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("NDC_DISCOVERY_FAILURE_MODE", "degrade"); }

        let mut context = ExecutionContext::default();
        let temp_dir = TempDir::new().unwrap();
        context.project_root = temp_dir.path().to_path_buf();
        let executor = Arc::new(Executor::new(context));

        let existing_file = temp_dir.path().join("main.rs");
        std::fs::write(&existing_file, "fn main() {}").unwrap();

        let task = executor
            .create_task(
                "Discovery signal".to_string(),
                "Should persist discovery fact".to_string(),
                AgentRole::Historian,
            )
            .await
            .unwrap();
        let mut stored = executor
            .context()
            .storage
            .get_task(&task.id)
            .await
            .unwrap()
            .unwrap();
        stored.intent = Some(Intent {
            id: IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Implementer,
            proposed_action: ndc_core::Action::ReadFile {
                path: existing_file,
            },
            effects: Vec::new(),
            reasoning: "trigger discovery".to_string(),
            task_id: Some(task.id),
            timestamp: chrono::Utc::now(),
        });
        executor.context().storage.save_task(&stored).await.unwrap();

        let result = executor.execute_task(task.id).await;
        assert!(result.is_ok());

        let mem_id = ndc_runtime::Executor::gold_memory_entry_id();
        let memory = executor
            .context()
            .storage
            .get_memory(&mem_id)
            .await
            .unwrap()
            .expect("gold memory entry should exist");
        match memory.content {
            ndc_core::MemoryContent::General { text, metadata } => {
                assert_eq!(metadata, "gold_memory_service/v2");
                let payload: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(payload.get("version").and_then(|v| v.as_u64()), Some(2));
            }
            _ => panic!("expected general memory payload"),
        }

        unsafe { std::env::remove_var("NDC_DISCOVERY_FAILURE_MODE"); }
    }

    /// P0.13 smoke: query persisted GoldMemory facts via ndc_memory_query
    #[tokio::test]
    async fn test_smoke_ndc_memory_query_tool() {
        let storage = create_memory_storage();
        let mut service = ndc_core::GoldMemoryService::new();
        service.upsert_system_fact(SystemFactInput {
            dedupe_key: "task:smoke:quality_gate_failed".to_string(),
            rule: "Quality gate must pass before completion".to_string(),
            description: "quality gate failed in smoke".to_string(),
            scope_pattern: "smoke-task".to_string(),
            priority: InvariantPriority::Critical,
            tags: vec!["verification".to_string(), "quality_gate".to_string()],
            evidence: vec!["kind=quality_gate_failed".to_string()],
            source: "verifier".to_string(),
        });

        let payload = serde_json::json!({
            "version": 2,
            "service": service
        });
        let entry = MemoryEntry {
            id: ndc_runtime::Executor::gold_memory_entry_id(),
            content: MemoryContent::General {
                text: serde_json::to_string(&payload).unwrap(),
                metadata: "gold_memory_service/v2".to_string(),
            },
            embedding: Vec::new(),
            relations: Vec::new(),
            metadata: MemoryMetadata {
                stability: MemoryStability::Canonical,
                created_at: chrono::Utc::now(),
                created_by: AgentId::system(),
                source_task: ndc_core::TaskId::new(),
                version: 2,
                modified_at: Some(chrono::Utc::now()),
                tags: vec!["gold-memory".to_string()],
            },
            access_control: AccessControl::new(AgentId::system(), MemoryStability::Canonical),
        };
        storage.save_memory(&entry).await.unwrap();

        let manager = create_default_tool_manager_with_storage(storage);
        let query = manager
            .execute(
                "ndc_memory_query",
                &serde_json::json!({
                    "priority": "critical",
                    "tags": "quality_gate",
                    "source": "system_inference"
                }),
            )
            .await
            .unwrap();
        assert!(query.success);
        assert!(query.output.contains("GoldMemory Query Results"));
        assert!(query
            .output
            .contains("Quality gate must pass before completion"));
    }

    /// Test task ID uniqueness
    #[tokio::test]
    async fn test_task_id_uniqueness() {
        let executor = create_test_executor();

        // Create multiple tasks
        let ids: Vec<_> = (0..10)
            .map(|_| {
                executor.create_task(
                    format!("Unique ID Test"),
                    "Test".to_string(),
                    AgentRole::Historian,
                )
            })
            .collect();

        let results = futures::future::join_all(ids).await;

        // Extract IDs
        let task_ids: Vec<_> = results
            .iter()
            .filter_map(|r| r.as_ref().ok().map(|t| t.id))
            .collect();

        // All IDs should be unique
        let unique_ids: std::collections::HashSet<_> = task_ids.iter().collect();
        assert_eq!(task_ids.len(), unique_ids.len());
    }

    /// Test task with metadata
    #[tokio::test]
    async fn test_task_metadata() {
        let executor = create_test_executor();

        // Create task with specific role
        let task = executor
            .create_task(
                "Metadata Test".to_string(),
                "Task with metadata".to_string(),
                AgentRole::Implementer,
            )
            .await
            .unwrap();

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
        let task = executor
            .create_task(
                "Result Structure Test".to_string(),
                "Test result structure".to_string(),
                AgentRole::Historian,
            )
            .await
            .unwrap();

        let result = executor.execute_task(task.id).await.unwrap();

        // Verify result structure
        assert!(result.success);
        assert_eq!(result.task_id, task.id);
        assert_eq!(result.final_state, TaskState::Completed);
        assert!(!result.output.is_empty());
        assert!(result.error.is_none());
        assert!(result.metrics.total_duration_ms < 60_000);
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
            let task = executor
                .create_task(
                    format!("Task by {:?}", role),
                    format!("Created by role {}", i),
                    *role,
                )
                .await
                .unwrap();

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
            "path": test_file.to_string_lossy(),
            "working_dir": temp_dir.path().to_string_lossy()
        });

        let result = fs_tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("test content"));
    }
}
