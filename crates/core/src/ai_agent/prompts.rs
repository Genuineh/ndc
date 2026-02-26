//! System Prompts for AI Agent
//!
//! 职责:
//! - 构建系统提示词
//! - 生成上下文感知的提示词
//! - 管理提示词模板
//! - 集成 Knowledge Injectors (WorkingMemory, Invariants, Lineage)

use super::injectors::invariant::InvariantInjector;
use super::injectors::lineage::LineageInjector;
use super::injectors::working_memory::WorkingMemoryInjector;
use serde_json::Value as JsonValue;

/// 增强的提示词上下文 - 包含知识注入
#[derive(Debug, Clone, Default)]
pub struct EnhancedPromptContext {
    /// 可用工具列表 (JSON Schema)
    pub available_tools: Vec<JsonValue>,

    /// 活跃任务 ID
    pub active_task_id: Option<crate::TaskId>,

    /// 工作目录
    pub working_dir: Option<std::path::PathBuf>,

    /// Working Memory Injector reference
    pub working_memory: Option<WorkingMemoryInjector>,

    /// Invariant Injector reference
    pub invariants: Option<InvariantInjector>,

    /// Lineage Injector reference
    pub lineage: Option<LineageInjector>,

    /// 当前任务模式匹配
    pub context_patterns: Vec<String>,
}

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
}

impl PromptBuilder {
    /// 创建新的提示词构建器
    pub fn new() -> Self {
        Self {
            base_template: include_str!("prompts/base_system.txt").to_string(),
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

        if let Some(working_dir) = context.working_dir.as_ref() {
            prompt.push_str("\n\n## Project Context\n");
            prompt.push_str(&format!(
                "Current working directory: {}\n",
                working_dir.display()
            ));
            prompt.push_str(
                "Treat this directory as the primary project scope for file/system operations.\n",
            );
        }

        prompt
    }

    /// 构建增强的系统提示词 - 集成 Knowledge Injectors
    pub fn build_enhanced(&self, context: &EnhancedPromptContext) -> String {
        let mut prompt = self.base_template.clone();

        // 1. 添加工具描述
        if !context.available_tools.is_empty() {
            let tools_desc = self.build_tools_description(&context.available_tools);
            prompt = prompt.replace("{{TOOLS}}", &tools_desc);
        } else {
            prompt = prompt.replace("{{TOOLS}}", "No tools available.");
        }

        if let Some(working_dir) = context.working_dir.as_ref() {
            prompt.push_str("\n\n## Project Context\n");
            prompt.push_str(&format!(
                "Current working directory: {}\n",
                working_dir.display()
            ));
            prompt.push_str(
                "Treat this directory as the primary project scope for file/system operations.\n",
            );
        }

        // 2. 注入 Working Memory
        if let Some(ref wm) = context.working_memory
            && wm.has_context()
        {
            let wm_injection = wm.inject();
            prompt = format!("{}\n\n{}", wm_injection, prompt);
        }

        // 3. 注入 Invariants (Gold Memory)
        if let Some(ref inv) = context.invariants {
            let inv_injection = inv.inject(true, &context.context_patterns);
            prompt = format!("{}\n\n{}", inv_injection, prompt);
        }

        // 4. 注入 Task Lineage
        if let Some(ref lineage) = context.lineage
            && let Some(ref task_id) = context.active_task_id
        {
            let lineage_injection = lineage.inject(&task_id.to_string());
            prompt = format!("{}\n\n{}", lineage_injection, prompt);
        }

        prompt
    }

    /// 构建仅包含知识注入的提示词片段
    pub fn build_knowledge_injection(&self, context: &EnhancedPromptContext) -> String {
        let mut parts: Vec<String> = Vec::new();

        // Working Memory
        if let Some(ref wm) = context.working_memory
            && wm.has_context()
        {
            parts.push(wm.inject());
        }

        // Invariants
        if let Some(ref inv) = context.invariants {
            parts.push(inv.inject(true, &context.context_patterns));
        }

        // Lineage
        if let Some(ref lineage) = context.lineage
            && let Some(ref task_id) = context.active_task_id
        {
            parts.push(lineage.inject(&task_id.to_string()));
        }

        parts.join("\n\n")
    }

    /// 构建工具描述
    fn build_tools_description(&self, tools: &[JsonValue]) -> String {
        let mut desc = String::from("## Available Tools\n\n");

        for tool in tools {
            if let Some(function) = tool.get("function") {
                let name = function
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                let description = function
                    .get("description")
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

/// 构建增强的系统提示词 (便捷函数)
pub fn build_enhanced_prompt(context: &EnhancedPromptContext) -> String {
    let builder = PromptBuilder::new();
    builder.build_enhanced(context)
}

/// 构建知识注入片段 (便捷函数)
pub fn build_knowledge_injection(context: &EnhancedPromptContext) -> String {
    let builder = PromptBuilder::new();
    builder.build_knowledge_injection(context)
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
    fn test_build_system_prompt_includes_working_dir_context() {
        let context = PromptContext {
            available_tools: vec![],
            active_task_id: None,
            working_dir: Some(std::path::PathBuf::from("/tmp/demo")),
        };
        let prompt = build_system_prompt(&context);
        assert!(prompt.contains("Project Context"));
        assert!(prompt.contains("/tmp/demo"));
    }

    #[test]
    fn test_build_continuation_prompt() {
        let feedback = build_continuation_prompt("Test failed");
        assert!(feedback.contains("Test failed"));
        assert!(feedback.contains("continue working"));
    }

    #[test]
    fn test_enhanced_prompt_context_default() {
        let ctx = EnhancedPromptContext::default();
        assert!(ctx.available_tools.is_empty());
        assert!(ctx.context_patterns.is_empty());
    }

    #[test]
    fn test_build_enhanced_prompt_with_injectors() {
        use crate::ai_agent::injectors::invariant::{
            InvariantEntry, InvariantInjector, InvariantPriority,
        };

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

        let mut invariant_injector = InvariantInjector::default();
        invariant_injector.add_invariant(InvariantEntry::new(
            "test-invariant".to_string(),
            "Always validate input".to_string(),
            InvariantPriority::High,
        ));

        let context = EnhancedPromptContext {
            available_tools: vec![tool],
            active_task_id: None,
            working_dir: None,
            working_memory: None,
            invariants: Some(invariant_injector),
            lineage: None,
            context_patterns: vec!["test".to_string()],
        };

        let prompt = build_enhanced_prompt(&context);
        assert!(!prompt.is_empty());
        assert!(prompt.contains("test_tool"));
        assert!(prompt.contains("GOLD MEMORY INVARIANTS"));
    }

    #[test]
    fn test_build_knowledge_injection_empty() {
        let context = EnhancedPromptContext::default();
        let injection = build_knowledge_injection(&context);
        assert!(injection.is_empty());
    }
}
