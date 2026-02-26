//! Task Verify Tool - Verify task completion status
//!
//! Allows AI to verify if a task is actually completed.

use async_trait::async_trait;
use ndc_core::TaskId;

use super::super::schema::ToolSchemaBuilder;
use super::super::{Tool, ToolError, ToolMetadata, ToolResult};
use ndc_storage::{SharedStorage, create_memory_storage};

/// Task Verify Tool - 验证任务完成状态
#[derive(Clone)]
pub struct TaskVerifyTool {
    storage: SharedStorage,
}

impl TaskVerifyTool {
    pub fn new() -> Self {
        Self::with_storage(create_memory_storage())
    }

    pub fn with_storage(storage: SharedStorage) -> Self {
        Self { storage }
    }
}

impl Default for TaskVerifyTool {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TaskVerifyTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskVerifyTool").finish()
    }
}

#[async_trait]
impl Tool for TaskVerifyTool {
    fn name(&self) -> &str {
        "ndc_task_verify"
    }

    fn description(&self) -> &str {
        "Verify if a task is actually completed by checking its state, execution steps, and quality gates. Returns detailed verification status."
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let start = std::time::Instant::now();

        let task_id_str = params
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'task_id' parameter".to_string()))?;

        let task_id: TaskId = task_id_str
            .parse()
            .map_err(|_| ToolError::InvalidArgument(format!("Invalid task_id: {}", task_id_str)))?;

        let detailed = params
            .get("detailed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let task = self
            .storage
            .get_task(&task_id)
            .await
            .map_err(ToolError::ExecutionFailed)?
            .ok_or_else(|| ToolError::ExecutionFailed(format!("Task not found: {}", task_id)))?;

        let state_ok = task.state == ndc_core::TaskState::Completed;
        let failed_steps: Vec<&ndc_core::ExecutionStep> = task
            .steps
            .iter()
            .filter(|step| {
                step.status == ndc_core::StepStatus::Failed
                    || step.result.as_ref().map(|r| !r.success).unwrap_or(false)
            })
            .collect();
        let verified = state_ok && failed_steps.is_empty();

        let mut output = String::from("🔍 Task Verification Report\n\n");
        output.push_str(&format!("Task ID: {}\n", task_id));
        output.push_str(&format!("Title: {}\n", task.title));
        output.push_str(&format!("State: {:?}\n\n", task.state));

        output.push_str("Checks:\n");
        output.push_str(&format!(
            "- State is Completed: {}\n",
            if state_ok { "✓" } else { "✗" }
        ));
        output.push_str(&format!(
            "- No failed execution steps: {}\n",
            if failed_steps.is_empty() {
                "✓"
            } else {
                "✗"
            }
        ));
        output.push_str(&format!(
            "- Quality gate present: {}\n\n",
            if task.quality_gate.is_some() {
                "Yes"
            } else {
                "No"
            }
        ));

        if detailed {
            output.push_str("Execution Steps:\n");
            if task.steps.is_empty() {
                output.push_str("- (none)\n");
            } else {
                for step in &task.steps {
                    let step_ok = step.result.as_ref().map(|r| r.success).unwrap_or(true);
                    output.push_str(&format!(
                        "- Step {} {:?} result_ok={}\n",
                        step.step_id, step.status, step_ok
                    ));
                }
            }
            output.push('\n');
        }

        if verified {
            output.push_str("✅ Task verification PASSED");
        } else {
            output.push_str("❌ Task verification FAILED\n");
            if !state_ok {
                output.push_str("- Task is not in Completed state.\n");
            }
            if !failed_steps.is_empty() {
                output.push_str("- One or more execution steps failed.\n");
            }
        }

        let duration = start.elapsed().as_millis() as u64;
        tracing::info!(task_id = %task_id, verified, "Task verified via ndc_task_verify tool");

        Ok(ToolResult {
            success: verified,
            output,
            error: if verified {
                None
            } else {
                Some("Task verification failed".to_string())
            },
            metadata: ToolMetadata {
                execution_time_ms: duration,
                files_read: 0,
                files_written: 0,
                bytes_processed: 0,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        ToolSchemaBuilder::new()
            .description("Verify task completion status")
            .required_string("task_id", "The ID of the task to verify (ULID string)")
            .param_boolean("detailed", "Return detailed verification information including step-by-step analysis (default: false)")
            .build()
            .to_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndc_core::{AgentRole, Task};
    use serde_json::json;

    async fn seed_completed_task(storage: SharedStorage) -> Task {
        let mut task = Task::new(
            "Test".to_string(),
            "Description".to_string(),
            AgentRole::Implementer,
        );
        task.state = ndc_core::TaskState::Completed;
        storage.save_task(&task).await.unwrap();
        task
    }

    #[tokio::test]
    async fn test_task_verify_basic() {
        let storage = create_memory_storage();
        let task = seed_completed_task(storage.clone()).await;
        let tool = TaskVerifyTool::with_storage(storage);

        let params = json!({
            "task_id": task.id.to_string()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Verification Report"));
        assert!(result.output.contains("PASSED"));
    }

    #[tokio::test]
    async fn test_task_verify_with_detailed() {
        let storage = create_memory_storage();
        let task = seed_completed_task(storage.clone()).await;
        let tool = TaskVerifyTool::with_storage(storage);

        let params = json!({
            "task_id": task.id.to_string(),
            "detailed": true
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Execution Steps"));
    }

    #[tokio::test]
    async fn test_task_verify_missing_task_id() {
        let tool = TaskVerifyTool::new();
        let params = json!({});

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_verify_invalid_task_id() {
        let tool = TaskVerifyTool::new();
        let params = json!({
            "task_id": "invalid-ulid"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_verify_schema() {
        let tool = TaskVerifyTool::new();
        let schema = tool.schema();

        assert!(schema.is_object());
        let props = schema.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("task_id"));
        assert!(props.contains_key("detailed"));

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("task_id")));
    }

    #[tokio::test]
    async fn test_task_verify_name() {
        let tool = TaskVerifyTool::new();
        assert_eq!(tool.name(), "ndc_task_verify");
    }

    #[tokio::test]
    async fn test_task_verify_description() {
        let tool = TaskVerifyTool::new();
        assert!(tool.description().contains("Verify"));
        assert!(tool.description().contains("quality gates"));
    }
}
