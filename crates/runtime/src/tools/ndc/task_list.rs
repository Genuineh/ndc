//! Task List Tool - List and query NDC tasks
//!
//! Allows AI to list tasks with filtering options.

use async_trait::async_trait;
use serde_json::json;

use super::super::{Tool, ToolResult, ToolError, ToolMetadata};
use super::super::schema::ToolSchemaBuilder;

/// Task List Tool - 列出任务
#[derive(Debug, Clone)]
pub struct TaskListTool;

impl TaskListTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskListTool {
    fn default() -> Self {
        Self::new()
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

        // 提取过滤参数
        let state_filter = params.get("state").and_then(|v| v.as_str());
        let priority_filter = params.get("priority").and_then(|v| v.as_str());
        let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20);
        let created_by = params.get("created_by").and_then(|v| v.as_str());
        let search_query = params.get("search").and_then(|v| v.as_str());

        // TODO: 从存储查询任务
        // 目前返回模拟数据

        // 构建输出
        let mut output = String::new();

        if let Some(query) = search_query {
            output.push_str(&format!("📋 Tasks matching '{}':\n\n", query));
        } else {
            output.push_str("📋 NDC Tasks:\n\n");
        }

        // 显示应用的过滤器
        let mut filters = Vec::new();
        if let Some(state) = state_filter {
            filters.push(format!("state={}", state));
        }
        if let Some(priority) = priority_filter {
            filters.push(format!("priority={}", priority));
        }
        if let Some(creator) = created_by {
            filters.push(format!("created_by={}", creator));
        }
        if !filters.is_empty() {
            output.push_str(&format!("Filters: {}\n\n", filters.join(", ")));
        }

        // 模拟任务列表 (TODO: 实际查询)
        output.push_str("(Task listing would appear here - TODO: integrate with storage)\n");

        // 显示查询提示
        if search_query.is_none() && state_filter.is_none() && priority_filter.is_none() {
            output.push_str("\n💡 Tip: Use filters to narrow results:\n");
            output.push_str("  - state: pending, in_progress, completed, etc.\n");
            output.push_str("  - priority: low, normal, high, critical\n");
            output.push_str("  - search: keyword search in title/description\n");
            output.push_str("  - limit: maximum number of results (default: 20)\n");
        }

        let duration = start.elapsed().as_millis() as u64;

        tracing::info!(
            filters = filters.len(),
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

    #[tokio::test]
    async fn test_task_list_all() {
        let tool = TaskListTool::new();
        let params = json!({});

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("NDC Tasks"));
    }

    #[tokio::test]
    async fn test_task_list_with_state_filter() {
        let tool = TaskListTool::new();
        let params = json!({
            "state": "pending"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("state=pending"));
    }

    #[tokio::test]
    async fn test_task_list_with_priority_filter() {
        let tool = TaskListTool::new();
        let params = json!({
            "priority": "high"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("priority=high"));
    }

    #[tokio::test]
    async fn test_task_list_with_search() {
        let tool = TaskListTool::new();
        let params = json!({
            "search": "auth"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("auth"));
    }

    #[tokio::test]
    async fn test_task_list_with_limit() {
        let tool = TaskListTool::new();
        let params = json!({
            "limit": 5
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_task_list_multiple_filters() {
        let tool = TaskListTool::new();
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
