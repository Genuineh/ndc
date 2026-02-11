//! System Prompts for AI Agent
//!
//! 职责:
//! - 构建系统提示词
//! - 生成上下文感知的提示词
//! - 管理提示词模板

use serde_json::Value as JsonValue;

/// 提示词上下文
#[derive(Debug, Clone)]
pub struct PromptContext {
    /// 可用工具列表 (JSON Schema)
    pub available_tools: Vec<JsonValue>,

    /// 活跃任务 ID
    pub active_task_id: Option<crate::TaskId>,

    /// 工作目录
    pub working_dir: Option<std::path::PathBuf>,
}

/// 提示词构建器
pub struct PromptBuilder {
    /// 基础模板
    base_template: String,

    /// 工具描述模板
    tools_template: String,

    /// 反馈模板
    feedback_template: String,
}

impl PromptBuilder {
    /// 创建新的提示词构建器
    pub fn new() -> Self {
        Self {
            base_template: include_str!("prompts/base_system.txt").to_string(),
            tools_template: include_str!("prompts/tools_description.txt").to_string(),
            feedback_template: include_str!("prompts/feedback.txt").to_string(),
        }
    }

    /// 构建系统提示词
    pub fn build(&self, context: &PromptContext) -> String {
        let mut prompt = self.base_template.clone();

        // 添加工具描述
        if !context.available_tools.is_empty() {
            let tools_desc = self.build_tools_description(&context.available_tools);
            prompt = prompt.replace("{{TOOLS}}", &tools_desc);
        } else {
            prompt = prompt.replace("{{TOOLS}}", "No tools available.");
        }

        prompt
    }

    /// 构建工具描述
    fn build_tools_description(&self, tools: &[JsonValue]) -> String {
        let mut desc = String::from("## Available Tools\n\n");

        for tool in tools {
            if let Some(function) = tool.get("function") {
                let name = function.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                let description = function.get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("No description");

                desc.push_str(&format!("### {}\n{}\n\n", name, description));
            }
        }

        desc
    }
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 构建系统提示词 (便捷函数)
pub fn build_system_prompt(context: &PromptContext) -> String {
    let builder = PromptBuilder::new();
    builder.build(context)
}

/// 构建基础系统提示词 (无上下文)
pub fn build_base_system_prompt() -> String {
    include_str!("prompts/base_system.txt").to_string()
}

/// 构建继续指令
pub fn build_continuation_prompt(feedback: &str) -> String {
    format!(
        "## Feedback\n\n{}\n\nPlease continue working on the task and address the issues above.",
        feedback
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_builder_new() {
        let builder = PromptBuilder::new();
        assert!(!builder.base_template.is_empty());
    }

    #[test]
    fn test_build_base_system_prompt() {
        let prompt = build_base_system_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("NDC"));
    }

    #[test]
    fn test_build_system_prompt_no_tools() {
        let context = PromptContext {
            available_tools: vec![],
            active_task_id: None,
            working_dir: None,
        };

        let prompt = build_system_prompt(&context);
        assert!(!prompt.is_empty());
        assert!(prompt.contains("No tools available"));
    }

    #[test]
    fn test_build_system_prompt_with_tools() {
        let tool = serde_json::json!({
            "type": "function",
            "function": {
                "name": "test_tool",
                "description": "A test tool",
                "parameters": {
                    "type": "object",
                    "properties": {}
                }
            }
        });

        let context = PromptContext {
            available_tools: vec![tool],
            active_task_id: None,
            working_dir: None,
        };

        let prompt = build_system_prompt(&context);
        assert!(!prompt.is_empty());
        assert!(prompt.contains("test_tool"));
        assert!(prompt.contains("A test tool"));
    }

    #[test]
    fn test_build_continuation_prompt() {
        let feedback = build_continuation_prompt("Test failed");
        assert!(feedback.contains("Test failed"));
        assert!(feedback.contains("continue working"));
    }
}
