//! Task Update Tool - Update existing NDC tasks
//!
//! Allows AI to update task status, add notes, change priority, etc.

use async_trait::async_trait;
use ndc_core::{TaskId, TaskPriority, TaskState};

use super::super::schema::{JsonSchema, JsonSchemaProperty, ToolSchemaBuilder};
use super::super::{Tool, ToolError, ToolMetadata, ToolResult};
use ndc_storage::{SharedStorage, create_memory_storage};

/// Task Update Tool - 更新任务状态
#[derive(Clone)]
pub struct TaskUpdateTool {
    storage: SharedStorage,
}

impl TaskUpdateTool {
    pub fn new() -> Self {
        Self::with_storage(create_memory_storage())
    }

    pub fn with_storage(storage: SharedStorage) -> Self {
        Self { storage }
    }
}

fn parse_state(value: &str) -> Option<TaskState> {
    match value.to_ascii_lowercase().as_str() {
        "pending" => Some(TaskState::Pending),
        "preparing" => Some(TaskState::Preparing),
        "in_progress" | "in-progress" => Some(TaskState::InProgress),
        "awaiting_verification" | "awaiting-verification" => Some(TaskState::AwaitingVerification),
        "blocked" => Some(TaskState::Blocked),
        "completed" => Some(TaskState::Completed),
        "failed" => Some(TaskState::Failed),
        "cancelled" | "canceled" => Some(TaskState::Cancelled),
        _ => None,
    }
}

fn parse_priority(value: &str) -> Option<TaskPriority> {
    match value.to_ascii_lowercase().as_str() {
        "low" => Some(TaskPriority::Low),
        "medium" | "normal" => Some(TaskPriority::Medium),
        "high" | "urgent" => Some(TaskPriority::High),
        "critical" => Some(TaskPriority::Critical),
        _ => None,
    }
}

impl Default for TaskUpdateTool {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TaskUpdateTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskUpdateTool").finish()
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

        let task_id_str = params
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'task_id' parameter".to_string()))?;

        let task_id: TaskId = task_id_str
            .parse()
            .map_err(|_| ToolError::InvalidArgument(format!("Invalid task_id: {}", task_id_str)))?;

        let mut task = self
            .storage
            .get_task(&task_id)
            .await
            .map_err(ToolError::ExecutionFailed)?
            .ok_or_else(|| ToolError::ExecutionFailed(format!("Task not found: {}", task_id)))?;

        let mut updates = Vec::new();

        if let Some(state_str) = params.get("state").and_then(|v| v.as_str()) {
            let state = match parse_state(state_str) {
                Some(state) => state,
                None => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Invalid state: {}", state_str)),
                        metadata: ToolMetadata::default(),
                    });
                }
            };

            if task.state != state {
                let from_state = task.state.clone();
                if let Err(_transition_error) = task.request_transition(state.clone()) {
                    let allowed = task
                        .allowed_transitions
                        .iter()
                        .map(|s| format!("{:?}", s))
                        .collect::<Vec<_>>();
                    let allowed_text = if allowed.is_empty() {
                        "<none>".to_string()
                    } else {
                        allowed.join(", ")
                    };
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!(
                            "Invalid state transition: {:?} -> {:?}. Allowed transitions: {}",
                            from_state, state, allowed_text
                        )),
                        metadata: ToolMetadata::default(),
                    });
                }
                updates.push(format!("state: {:?}", state));
            }
        }

        if let Some(priority_str) = params.get("priority").and_then(|v| v.as_str()) {
            let priority = parse_priority(priority_str).unwrap_or(TaskPriority::Medium);
            task.metadata.priority = priority;
            updates.push(format!("priority: {:?}", priority));
        }

        if let Some(notes) = params.get("notes").and_then(|v| v.as_str())
            && !notes.trim().is_empty()
        {
            if !task.description.is_empty() {
                task.description.push_str("\n\n");
            }
            task.description.push_str(&format!(
                "[note {}] {}",
                chrono::Utc::now().to_rfc3339(),
                notes.trim()
            ));
            updates.push(format!("notes: {} chars", notes.len()));
        }

        if let Some(add_tags) = params.get("add_tags").and_then(|v| v.as_array()) {
            let tags: Vec<String> = add_tags
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect();
            if !tags.is_empty() {
                for tag in &tags {
                    if !task.metadata.tags.iter().any(|existing| existing == tag) {
                        task.metadata.tags.push(tag.clone());
                    }
                }
                updates.push(format!("tags: {}", tags.join(", ")));
            }
        }

        if updates.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(
                    "No updates specified. Provide at least one of: state, priority, notes, add_tags"
                        .to_string(),
                ),
                metadata: ToolMetadata::default(),
            });
        }

        task.metadata.updated_at = chrono::Utc::now();
        self.storage
            .save_task(&task)
            .await
            .map_err(ToolError::ExecutionFailed)?;

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
        let string_schema = JsonSchemaProperty::string("A tag string").to_value();
        let items_schema: JsonSchema =
            serde_json::from_value(string_schema).unwrap_or_else(|_| JsonSchema::object());

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
    use ndc_core::{AgentRole, Task};
    use serde_json::json;

    async fn seed_task(storage: SharedStorage) -> Task {
        let task = Task::new(
            "Test".to_string(),
            "Description".to_string(),
            AgentRole::Implementer,
        );
        storage.save_task(&task).await.unwrap();
        task
    }

    #[tokio::test]
    async fn test_task_update_state() {
        let storage = create_memory_storage();
        let task = seed_task(storage.clone()).await;
        let tool = TaskUpdateTool::with_storage(storage);
        let task_id = task.id.to_string();

        let params = json!({
            "task_id": task_id,
            "state": "preparing"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("state: Preparing"));
        assert!(result.output.contains("updated successfully"));
    }

    #[tokio::test]
    async fn test_task_update_priority() {
        let storage = create_memory_storage();
        let task = seed_task(storage.clone()).await;
        let tool = TaskUpdateTool::with_storage(storage);
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
        let storage = create_memory_storage();
        let task = seed_task(storage.clone()).await;
        let tool = TaskUpdateTool::with_storage(storage);
        let task_id = task.id.to_string();

        let params = json!({
            "task_id": task_id,
            "state": "preparing",
            "priority": "urgent",
            "notes": "Working on this"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("priority: High"));
        assert!(result.output.contains("notes:"));
    }

    #[tokio::test]
    async fn test_task_update_rejects_invalid_transition() {
        let storage = create_memory_storage();
        let task = seed_task(storage.clone()).await;
        let tool = TaskUpdateTool::with_storage(storage.clone());
        let task_id = task.id.to_string();

        let params = json!({
            "task_id": task_id,
            "state": "completed"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(!result.success);
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("Invalid state transition")
        );

        let persisted = storage.get_task(&task.id).await.unwrap().unwrap();
        assert_eq!(persisted.state, TaskState::Pending);
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
        let storage = create_memory_storage();
        let task = seed_task(storage.clone()).await;
        let tool = TaskUpdateTool::with_storage(storage);
        let task_id = task.id.to_string();

        let params = json!({
            "task_id": task_id
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(!result.success);
        assert!(
            result
                .error
                .as_ref()
                .unwrap()
                .contains("No updates specified")
        );
    }

    #[tokio::test]
    async fn test_task_update_invalid_state() {
        let storage = create_memory_storage();
        let task = seed_task(storage.clone()).await;
        let tool = TaskUpdateTool::with_storage(storage);
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

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("task_id")));
    }
}
