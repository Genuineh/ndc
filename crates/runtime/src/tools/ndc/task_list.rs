//! Task List Tool - List and query NDC tasks
//!
//! Allows AI to list tasks with filtering options.

use async_trait::async_trait;
use ndc_core::{AgentRole, Task, TaskPriority, TaskState};

use super::super::schema::ToolSchemaBuilder;
use super::super::{Tool, ToolError, ToolMetadata, ToolResult};
use ndc_storage::{create_memory_storage, SharedStorage};

/// Task List Tool - 列出任务
#[derive(Clone)]
pub struct TaskListTool {
    storage: SharedStorage,
}

impl TaskListTool {
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

fn parse_role(value: &str) -> Option<AgentRole> {
    match value.to_ascii_lowercase().as_str() {
        "planner" => Some(AgentRole::Planner),
        "implementer" => Some(AgentRole::Implementer),
        "reviewer" => Some(AgentRole::Reviewer),
        "tester" => Some(AgentRole::Tester),
        "historian" => Some(AgentRole::Historian),
        "admin" => Some(AgentRole::Admin),
        _ => None,
    }
}

fn matches_filters(
    task: &Task,
    state_filter: Option<&TaskState>,
    priority_filter: Option<&TaskPriority>,
    created_by_filter: Option<&AgentRole>,
    search_filter: Option<&str>,
) -> bool {
    if let Some(state) = state_filter
        && &task.state != state {
            return false;
        }
    if let Some(priority) = priority_filter
        && &task.metadata.priority != priority {
            return false;
        }
    if let Some(created_by) = created_by_filter
        && &task.metadata.created_by != created_by {
            return false;
        }
    if let Some(query) = search_filter {
        let query = query.to_ascii_lowercase();
        let hay = format!("{} {}", task.title, task.description).to_ascii_lowercase();
        if !hay.contains(&query) {
            return false;
        }
    }
    true
}

impl Default for TaskListTool {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TaskListTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskListTool").finish()
    }
}

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str {
        "ndc_task_list"
    }

    fn description(&self) -> &str {
        "List and query NDC tasks with optional filtering by state, priority, and other criteria. Use this to see what tasks exist or find specific tasks."
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let start = std::time::Instant::now();

        let state_filter_raw = params.get("state").and_then(|v| v.as_str());
        let priority_filter_raw = params.get("priority").and_then(|v| v.as_str());
        let created_by_raw = params.get("created_by").and_then(|v| v.as_str());
        let search_query = params.get("search").and_then(|v| v.as_str());
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(100) as usize;

        let state_filter = match state_filter_raw {
            Some(state) => Some(parse_state(state).ok_or_else(|| {
                ToolError::InvalidArgument(format!("Invalid state filter: {}", state))
            })?),
            None => None,
        };
        let priority_filter = match priority_filter_raw {
            Some(priority) => Some(parse_priority(priority).ok_or_else(|| {
                ToolError::InvalidArgument(format!("Invalid priority filter: {}", priority))
            })?),
            None => None,
        };
        let created_by_filter = match created_by_raw {
            Some(role) => Some(parse_role(role).ok_or_else(|| {
                ToolError::InvalidArgument(format!("Invalid created_by filter: {}", role))
            })?),
            None => None,
        };

        let mut tasks = self
            .storage
            .list_tasks()
            .await
            .map_err(ToolError::ExecutionFailed)?;
        tasks.sort_by_key(|task| std::cmp::Reverse(task.metadata.updated_at));

        let filtered: Vec<Task> = tasks
            .into_iter()
            .filter(|task| {
                matches_filters(
                    task,
                    state_filter.as_ref(),
                    priority_filter.as_ref(),
                    created_by_filter.as_ref(),
                    search_query,
                )
            })
            .take(limit)
            .collect();

        let mut output = String::new();
        if let Some(query) = search_query {
            output.push_str(&format!("📋 Tasks matching '{}':\n\n", query));
        } else {
            output.push_str("📋 NDC Tasks:\n\n");
        }

        let mut filters = Vec::new();
        if let Some(state) = state_filter_raw {
            filters.push(format!("state={}", state));
        }
        if let Some(priority) = priority_filter_raw {
            filters.push(format!("priority={}", priority));
        }
        if let Some(creator) = created_by_raw {
            filters.push(format!("created_by={}", creator));
        }
        if !filters.is_empty() {
            output.push_str(&format!("Filters: {}\n\n", filters.join(", ")));
        }

        if filtered.is_empty() {
            output.push_str("No tasks found.\n");
        } else {
            for task in &filtered {
                output.push_str(&format!(
                    "- {} [{}] ({:?}) {}\n",
                    task.id,
                    format!("{:?}", task.state),
                    task.metadata.priority,
                    task.title
                ));
            }
        }

        if search_query.is_none() && state_filter_raw.is_none() && priority_filter_raw.is_none() {
            output.push_str("\n💡 Tip: Use filters to narrow results:\n");
            output.push_str("  - state: pending, in_progress, completed, etc.\n");
            output.push_str("  - priority: low, normal, high, critical\n");
            output.push_str("  - search: keyword search in title/description\n");
            output.push_str("  - limit: maximum number of results (default: 20)\n");
        }

        let duration = start.elapsed().as_millis() as u64;

        tracing::info!(
            filters = filters.len(),
            tasks_returned = filtered.len(),
            limit = limit,
            "Tasks listed via ndc_task_list tool"
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
        ToolSchemaBuilder::new()
            .description("List and query NDC tasks")
            .param_string("state", "Filter by task state: pending, preparing, in_progress, awaiting_verification, blocked, completed, failed, cancelled")
            .param_string("priority", "Filter by task priority: low, normal, high, critical")
            .param_string("created_by", "Filter by creator role: planner, implementer, reviewer, tester, historian, admin")
            .param_string("search", "Search for tasks by keyword in title or description")
            .param_integer("limit", "Maximum number of tasks to return (default: 20, max: 100)")
            .build()
            .to_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_task_list_all() {
        let storage = create_memory_storage();
        let task = Task::new(
            "Task A".to_string(),
            "Description".to_string(),
            AgentRole::Implementer,
        );
        storage.save_task(&task).await.unwrap();
        let tool = TaskListTool::with_storage(storage);
        let params = json!({});

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("NDC Tasks"));
        assert!(result.output.contains("Task A"));
    }

    #[tokio::test]
    async fn test_task_list_with_state_filter() {
        let storage = create_memory_storage();
        let mut task = Task::new(
            "Task B".to_string(),
            "Description".to_string(),
            AgentRole::Implementer,
        );
        task.state = TaskState::Pending;
        storage.save_task(&task).await.unwrap();
        let tool = TaskListTool::with_storage(storage);
        let params = json!({
            "state": "pending"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("state=pending"));
    }

    #[tokio::test]
    async fn test_task_list_with_priority_filter() {
        let storage = create_memory_storage();
        let mut task = Task::new(
            "Task C".to_string(),
            "Description".to_string(),
            AgentRole::Implementer,
        );
        task.metadata.priority = TaskPriority::High;
        storage.save_task(&task).await.unwrap();
        let tool = TaskListTool::with_storage(storage);
        let params = json!({
            "priority": "high"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("priority=high"));
    }

    #[tokio::test]
    async fn test_task_list_with_search() {
        let storage = create_memory_storage();
        let task = Task::new(
            "Implement auth flow".to_string(),
            "Add oauth login".to_string(),
            AgentRole::Implementer,
        );
        storage.save_task(&task).await.unwrap();
        let tool = TaskListTool::with_storage(storage);
        let params = json!({
            "search": "auth"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("auth"));
    }

    #[tokio::test]
    async fn test_task_list_with_limit() {
        let storage = create_memory_storage();
        for i in 0..3 {
            let task = Task::new(
                format!("Task {}", i),
                "Description".to_string(),
                AgentRole::Implementer,
            );
            storage.save_task(&task).await.unwrap();
        }
        let tool = TaskListTool::with_storage(storage);
        let params = json!({
            "limit": 5
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_task_list_multiple_filters() {
        let storage = create_memory_storage();
        let mut task = Task::new(
            "Task D".to_string(),
            "Description".to_string(),
            AgentRole::Implementer,
        );
        task.state = TaskState::InProgress;
        task.metadata.priority = TaskPriority::High;
        storage.save_task(&task).await.unwrap();
        let tool = TaskListTool::with_storage(storage);
        let params = json!({
            "state": "in_progress",
            "priority": "high",
            "limit": 10
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("state=in_progress"));
        assert!(result.output.contains("priority=high"));
    }

    #[tokio::test]
    async fn test_task_list_schema() {
        let tool = TaskListTool::new();
        let schema = tool.schema();

        assert!(schema.is_object());
        let props = schema.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("state"));
        assert!(props.contains_key("priority"));
        assert!(props.contains_key("search"));
        assert!(props.contains_key("limit"));
    }
}
