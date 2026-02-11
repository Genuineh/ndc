//! Task Update Tool - Update existing NDC tasks
//!
//! Allows AI to update task status, add notes, change priority, etc.

use async_trait::async_trait;
use ndc_core::{TaskId, TaskState, TaskPriority};
use serde_json::json;

use super::super::{Tool, ToolResult, ToolError, ToolMetadata};
use super::super::schema::{ToolSchemaBuilder, JsonSchema, JsonSchemaProperty};

/// Task Update Tool - 更新任务状态
#[derive(Debug, Clone)]
pub struct TaskUpdateTool;

impl TaskUpdateTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskUpdateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskUpdateTool {
    fn name(&self) -> &str {
        "ndc_task_update"
    }

    fn description(&self) -> &str {
        "Update an existing NDC task's status, priority, or other properties. Use this to mark tasks as complete, add notes, or change task details."
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let start = std::time::Instant::now();

        // 提取任务 ID
        let task_id_str = params.get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'task_id' parameter".to_string()))?;

        // 解析任务 ID
        let task_id: TaskId = task_id_str.parse()
            .map_err(|_| ToolError::InvalidArgument(format!("Invalid task_id: {}", task_id_str)))?;

        // TODO: 从存储获取任务
        // 目前为模拟实现

        // 提取更新参数
        let mut updates = Vec::new();

        if let Some(state_str) = params.get("state").and_then(|v| v.as_str()) {
            let state = match state_str.to_lowercase().as_str() {
                "pending" => TaskState::Pending,
                "preparing" => TaskState::Preparing,
                "in_progress" | "in-progress" => TaskState::InProgress,
                "awaiting_verification" | "awaiting-verification" => TaskState::AwaitingVerification,
                "blocked" => TaskState::Blocked,
                "completed" => TaskState::Completed,
                "failed" => TaskState::Failed,
                "cancelled" | "canceled" => TaskState::Cancelled,
                _ => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Invalid state: {}", state_str)),
                        metadata: ToolMetadata::default(),
                    });
                }
            };
            updates.push(format!("state: {:?}", state));
        }

        if let Some(priority_str) = params.get("priority").and_then(|v| v.as_str()) {
            let priority = match priority_str.to_lowercase().as_str() {
                "low" => TaskPriority::Low,
                "medium" | "normal" => TaskPriority::Medium,
                "high" | "urgent" => TaskPriority::High,
                "critical" => TaskPriority::Critical,
                _ => TaskPriority::Medium,
            };
            updates.push(format!("priority: {:?}", priority));
        }

        if let Some(notes) = params.get("notes").and_then(|v| v.as_str()) {
            if !notes.is_empty() {
                updates.push(format!("notes: {} chars", notes.len()));
            }
        }

        if let Some(add_tags) = params.get("add_tags").and_then(|v| v.as_array()) {
            let tags: Vec<&str> = add_tags.iter().filter_map(|v| v.as_str()).collect();
            if !tags.is_empty() {
                updates.push(format!("tags: {}", tags.join(", ")));
            }
        }

        if updates.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("No updates specified. Provide at least one of: state, priority, notes, add_tags".to_string()),
                metadata: ToolMetadata::default(),
            });
        }

        // 构建输出
        let mut output = format!("✅ Task {} updated successfully!\n\n", task_id);
        output.push_str("Updates applied:\n");
        for update in &updates {
            output.push_str(&format!("  - {}\n", update));
        }

        let duration = start.elapsed().as_millis() as u64;

        tracing::info!(
            task_id = %task_id,
            updates_count = updates.len(),
            "Task updated via ndc_task_update tool"
        );

        Ok(ToolResult {
            success: true,
            output,
            error: None,
            metadata: ToolMetadata {
                execution_time_ms: duration,
                files_read: 0,
                files_written: 0,
                bytes_processed: 0,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        // Create a string schema for array items
        let string_schema = JsonSchemaProperty::string("A tag string").to_value();
        let items_schema: JsonSchema = serde_json::from_value(string_schema)
            .unwrap_or_else(|_| JsonSchema::object());

        ToolSchemaBuilder::new()
            .description("Update an existing NDC task")
            .required_string("task_id", "The ID of the task to update (ULID string)")
            .param_string("state", "New task state: pending, preparing, in_progress, awaiting_verification, blocked, completed, failed, cancelled")
            .param_string("priority", "New task priority: low, normal, high, critical")
            .param_string("notes", "Additional notes to add to the task")
            .param_array("add_tags", "Tags to add to the task", items_schema)
            .build()
            .to_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndc_core::Task;

    #[tokio::test]
    async fn test_task_update_state() {
        let tool = TaskUpdateTool::new();

        // Create a mock task to get a valid ID
        let task = Task::new("Test".to_string(), "Description".to_string(), ndc_core::AgentRole::Implementer);
        let task_id = task.id.to_string();

        let params = json!({
            "task_id": task_id,
            "state": "completed"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("updated successfully"));
        assert!(result.output.contains("state: Completed"));
    }

    #[tokio::test]
    async fn test_task_update_priority() {
        let tool = TaskUpdateTool::new();

        let task = Task::new("Test".to_string(), "Description".to_string(), ndc_core::AgentRole::Implementer);
        let task_id = task.id.to_string();

        let params = json!({
            "task_id": task_id,
            "priority": "high"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("priority: High"));
    }

    #[tokio::test]
    async fn test_task_update_multiple_fields() {
        let tool = TaskUpdateTool::new();

        let task = Task::new("Test".to_string(), "Description".to_string(), ndc_core::AgentRole::Implementer);
        let task_id = task.id.to_string();

        let params = json!({
            "task_id": task_id,
            "state": "in_progress",
            "priority": "urgent",
            "notes": "Working on this"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("state: InProgress"));
        assert!(result.output.contains("priority: High"));
    }

    #[tokio::test]
    async fn test_task_update_missing_task_id() {
        let tool = TaskUpdateTool::new();
        let params = json!({
            "state": "completed"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_update_no_updates() {
        let tool = TaskUpdateTool::new();

        let task = Task::new("Test".to_string(), "Description".to_string(), ndc_core::AgentRole::Implementer);
        let task_id = task.id.to_string();

        let params = json!({
            "task_id": task_id
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("No updates specified"));
    }

    #[tokio::test]
    async fn test_task_update_invalid_state() {
        let tool = TaskUpdateTool::new();

        let task = Task::new("Test".to_string(), "Description".to_string(), ndc_core::AgentRole::Implementer);
        let task_id = task.id.to_string();

        let params = json!({
            "task_id": task_id,
            "state": "invalid_state"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("Invalid state"));
    }

    #[tokio::test]
    async fn test_task_update_schema() {
        let tool = TaskUpdateTool::new();
        let schema = tool.schema();

        assert!(schema.is_object());
        let props = schema.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("task_id"));
        assert!(props.contains_key("state"));
        assert!(props.contains_key("priority"));
        assert!(props.contains_key("notes"));
        assert!(props.contains_key("add_tags"));

        // task_id should be required
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("task_id")));
    }
}
