//! Task Create Tool - Create new NDC tasks
//!
//! Allows AI to create new tasks with title and description.

use async_trait::async_trait;
use ndc_core::{Task, AgentRole, TaskPriority};
use serde_json::json;

use super::super::{Tool, ToolResult, ToolError, ToolMetadata};
use super::super::schema::ToolSchemaBuilder;

/// Task Create Tool - 创建新任务
#[derive(Debug, Clone)]
pub struct TaskCreateTool {
    // TODO: Add storage reference when integrating
}

impl TaskCreateTool {
    pub fn new() -> Self {
        Self { }
    }
}

impl Default for TaskCreateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &str {
        "ndc_task_create"
    }

    fn description(&self) -> &str {
        "Create a new NDC task with title and description. Use this when the user wants to start working on something new."
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let start = std::time::Instant::now();

        // 提取参数
        let title = params.get("title")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'title' parameter".to_string()))?;

        // 验证标题长度
        if title.len() > 200 {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Title too long (max 200 characters)".to_string()),
                metadata: ToolMetadata::default(),
            });
        }

        let description = params.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let priority_str = params.get("priority")
            .and_then(|v| v.as_str())
            .unwrap_or("normal");

        // 解析优先级
        let priority = match priority_str.to_lowercase().as_str() {
            "low" => TaskPriority::Low,
            "medium" | "normal" => TaskPriority::Medium,
            "high" | "urgent" => TaskPriority::High,
            "critical" => TaskPriority::Critical,
            _ => TaskPriority::Medium,
        };

        // 获取创建者角色 (默认为 Implementer)
        let created_by = params.get("created_by")
            .and_then(|v| v.as_str())
            .and_then(|r| match r.to_lowercase().as_str() {
                "planner" => Some(AgentRole::Planner),
                "implementer" => Some(AgentRole::Implementer),
                "reviewer" => Some(AgentRole::Reviewer),
                "tester" => Some(AgentRole::Tester),
                "historian" => Some(AgentRole::Historian),
                "admin" => Some(AgentRole::Admin),
                _ => None,
            })
            .unwrap_or(AgentRole::Implementer);

        // 创建任务
        let mut task = Task::new(title.to_string(), description.to_string(), created_by);
        task.metadata.priority = priority;

        // TODO: 实际保存到存储
        // 目前返回模拟的任务 ID
        let task_id = task.id.to_string();

        // 构建任务摘要
        let mut output = format!("✅ Task created successfully!\n\n");
        output.push_str(&format!("Task ID: {}\n", task_id));
        output.push_str(&format!("Title: {}\n", task.title));
        if !description.is_empty() {
            output.push_str(&format!("Description: {}\n", task.description));
        }
        output.push_str(&format!("Priority: {:?}\n", task.metadata.priority));
        output.push_str(&format!("State: {:?}\n", task.state));

        let duration = start.elapsed().as_millis() as u64;

        tracing::info!(
            task_id = %task_id,
            title = %task.title,
            priority = ?priority,
            "Task created via ndc_task_create tool"
        );

        Ok(ToolResult {
            success: true,
            output,
            error: None,
            metadata: ToolMetadata {
                execution_time_ms: duration,
                files_read: 0,
                files_written: 0,
                bytes_processed: title.len() as u64 + description.len() as u64,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        ToolSchemaBuilder::new()
            .description("Create a new NDC task")
            .required_string("title", "Short task title (max 200 characters)")
            .param_string("description", "Detailed task description explaining what needs to be done")
            .param_string("priority", "Task priority: low, normal, high, or critical (default: normal)")
            .param_string("created_by", "Role creating this task: planner, implementer, reviewer, tester, historian, admin (default: implementer)")
            .build()
            .to_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_create_basic() {
        let tool = TaskCreateTool::new();
        let params = json!({
            "title": "Test task"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Test task"));
        assert!(result.output.contains("Task ID:"));
    }

    #[tokio::test]
    async fn test_task_create_with_description() {
        let tool = TaskCreateTool::new();
        let params = json!({
            "title": "Implement feature",
            "description": "Add new authentication endpoint"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Implement feature"));
        assert!(result.output.contains("Add new authentication endpoint"));
    }

    #[tokio::test]
    async fn test_task_create_with_priority() {
        let tool = TaskCreateTool::new();
        let params = json!({
            "title": "Urgent bug fix",
            "priority": "high"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("High"));
    }

    #[tokio::test]
    async fn test_task_create_title_too_long() {
        let tool = TaskCreateTool::new();
        let long_title = "a".repeat(201);
        let params = json!({
            "title": long_title
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("too long"));
    }

    #[tokio::test]
    async fn test_task_create_missing_title() {
        let tool = TaskCreateTool::new();
        let params = json!({
            "description": "Test"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
        match result {
            Err(ToolError::InvalidArgument(msg)) => {
                assert!(msg.contains("title"));
            }
            _ => panic!("Expected InvalidArgument error"),
        }
    }

    #[tokio::test]
    async fn test_task_create_schema() {
        let tool = TaskCreateTool::new();
        let schema = tool.schema();

        assert!(schema.is_object());
        let props = schema.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("title"));
        assert!(props.contains_key("description"));
        assert!(props.contains_key("priority"));

        // title should be required
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("title")));
    }
}
