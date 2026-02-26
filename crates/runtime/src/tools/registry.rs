//! Tool Registry - 工具注册表 + 动态加载
//!
//! 设计参考 OpenCode 的工具系统:
//! - 统一工具注册和管理
//! - 动态工具发现和注册
//! - 工具分类和分组
//! - 工具调用统计和审计

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

use super::trait_mod::{Tool, ToolError, ToolParams, ToolResult};

/// 工具注册表
#[derive(Default)]
pub struct ToolRegistry {
    /// 工具映射
    tools: HashMap<String, Arc<dyn Tool>>,

    /// 工具元数据
    metadata: HashMap<String, ToolMetadata>,

    /// 工具分类
    categories: HashMap<String, Vec<String>>,
}


impl ToolRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册工具
    pub fn register<T: Tool + 'static>(&mut self, tool: T) -> String {
        let name = tool.name().to_string();
        let description = tool.description().to_string();
        let schema = tool.schema();
        let tool = Arc::new(tool);

        self.tools.insert(name.clone(), tool);

        let metadata = ToolMetadata {
            name: name.clone(),
            description,
            schema,
            registered_at: chrono::Utc::now(),
            call_count: 0,
            success_count: 0,
            failure_count: 0,
        };
        self.metadata.insert(name.clone(), metadata);
        self.add_to_category("all", &name);

        debug!(tool = name, "Tool registered");
        name
    }

    /// 注册工具（带自定义名称）
    pub fn register_as<T: Tool + 'static>(&mut self, name: &str, tool: T) {
        let description = tool.description().to_string();
        let schema = tool.schema();
        let tool = Arc::new(tool);

        self.tools.insert(name.to_string(), tool);
        self.metadata.insert(
            name.to_string(),
            ToolMetadata {
                name: name.to_string(),
                description,
                schema,
                registered_at: chrono::Utc::now(),
                call_count: 0,
                success_count: 0,
                failure_count: 0,
            },
        );
        self.add_to_category("all", name);
    }

    /// 批量注册工具
    pub fn register_all(&mut self, tools: &[Arc<dyn Tool>]) {
        for tool in tools {
            self.register_ref(tool);
        }
    }

    /// 注册工具（通过引用）
    fn register_ref(&mut self, tool: &Arc<dyn Tool>) {
        let name = tool.name().to_string();
        let description = tool.description().to_string();
        let schema = tool.schema();

        self.tools.insert(name.clone(), tool.clone());

        let metadata = ToolMetadata {
            name: name.clone(),
            description,
            schema,
            registered_at: chrono::Utc::now(),
            call_count: 0,
            success_count: 0,
            failure_count: 0,
        };
        self.metadata.insert(name.clone(), metadata);
        self.add_to_category("all", &name);
    }

    /// 获取工具
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    /// 检查工具是否存在
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// 获取所有工具名称
    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// 获取所有工具
    pub fn all(&self) -> Vec<&Arc<dyn Tool>> {
        self.tools.values().collect()
    }

    /// 按类别获取工具
    pub fn by_category(&self, category: &str) -> Vec<&Arc<dyn Tool>> {
        if let Some(names) = self.categories.get(category) {
            names
                .iter()
                .filter_map(|name| self.tools.get(name))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 获取所有类别
    pub fn categories(&self) -> Vec<&str> {
        self.categories.keys().map(|s| s.as_str()).collect()
    }

    /// 添加到分类
    pub fn add_to_category(&mut self, category: &str, tool_name: &str) {
        self.categories
            .entry(category.to_string())
            .or_default()
            .push(tool_name.to_string());
    }

    /// 创建分类
    pub fn create_category(&mut self, category: &str) {
        self.categories
            .entry(category.to_string())
            .or_default();
    }

    /// 执行工具
    pub async fn execute(
        &mut self,
        name: &str,
        params: &ToolParams,
    ) -> Result<ToolResult, ToolError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::NotFound(name.to_string()))?;

        let start = std::time::Instant::now();
        let result = tool.execute(params).await;
        let _duration = start.elapsed().as_millis() as u64;

        if let Some(meta) = self.metadata.get_mut(name) {
            meta.call_count += 1;
            match &result {
                Ok(_) => meta.success_count += 1,
                Err(_) => meta.failure_count += 1,
            }
        }

        result
    }

    /// 获取工具元数据
    pub fn metadata(&self, name: &str) -> Option<&ToolMetadata> {
        self.metadata.get(name)
    }

    /// 获取所有元数据
    pub fn all_metadata(&self) -> Vec<&ToolMetadata> {
        self.metadata.values().collect()
    }

    /// 获取统计摘要
    pub fn summary(&self) -> RegistrySummary {
        let total_calls: u64 = self.metadata.values().map(|m| m.call_count).sum();
        let total_success: u64 = self.metadata.values().map(|m| m.success_count).sum();

        RegistrySummary {
            total_tools: self.metadata.len(),
            total_calls,
            success_rate: if total_calls > 0 {
                total_success as f64 / total_calls as f64
            } else {
                0.0
            },
            categories: self.categories.len(),
        }
    }

    /// 获取 LLM 友好的工具列表
    pub fn tool_list_for_llm(&self) -> String {
        let mut lines = Vec::new();

        for (name, tool) in &self.tools {
            lines.push(format!("- {}: {}", name, tool.description()));
        }

        lines.join("\n")
    }

    /// 生成工具 Schema 列表
    pub fn schemas_for_llm(&self) -> Vec<(String, serde_json::Value)> {
        self.tools
            .iter()
            .map(|(name, tool)| (name.clone(), tool.schema()))
            .collect()
    }
}

/// 工具元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    /// 工具名称
    pub name: String,

    /// 工具描述
    pub description: String,

    /// 工具 Schema
    pub schema: serde_json::Value,

    /// 注册时间
    pub registered_at: chrono::DateTime<chrono::Utc>,

    /// 调用次数
    pub call_count: u64,

    /// 成功次数
    pub success_count: u64,

    /// 失败次数
    pub failure_count: u64,
}

impl ToolMetadata {
    /// 成功率
    pub fn success_rate(&self) -> f64 {
        if self.call_count == 0 {
            0.0
        } else {
            self.success_count as f64 / self.call_count as f64
        }
    }
}

/// 注册表统计摘要
#[derive(Debug, Clone)]
pub struct RegistrySummary {
    /// 工具总数
    pub total_tools: usize,

    /// 调用总数
    pub total_calls: u64,

    /// 成功率
    pub success_rate: f64,

    /// 类别数
    pub categories: usize,
}

/// 预定义工具类别
pub struct PredefinedCategories;

impl PredefinedCategories {
    pub const FILE_OPS: &'static str = "file_operations";
    pub const GIT_OPS: &'static str = "git_operations";
    pub const SHELL_OPS: &'static str = "shell_operations";
    pub const SEARCH: &'static str = "search";
    pub const WEB: &'static str = "web";
    pub const ANALYSIS: &'static str = "analysis";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::schema::ToolSchemaBuilder;

    // 测试工具实现
    struct TestTool;

    #[async_trait::async_trait]
    impl Tool for TestTool {
        fn name(&self) -> &str {
            "test_tool"
        }

        fn description(&self) -> &str {
            "A test tool"
        }

        async fn execute(&self, _params: &ToolParams) -> Result<ToolResult, ToolError> {
            Ok(ToolResult {
                success: true,
                output: "test output".to_string(),
                error: None,
                metadata: super::super::trait_mod::ToolMetadata {
                    execution_time_ms: 10,
                    files_read: 0,
                    files_written: 0,
                    bytes_processed: 0,
                },
            })
        }

        fn schema(&self) -> serde_json::Value {
            ToolSchemaBuilder::new()
                .description("A test tool")
                .required_string("name", "The name")
                .build()
                .to_value()
        }
    }

    #[tokio::test]
    async fn test_registry_new() {
        let registry = ToolRegistry::new();
        let summary = registry.summary();
        assert_eq!(summary.total_tools, 0);
    }

    #[tokio::test]
    async fn test_registry_register() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        assert!(registry.contains("test_tool"));
        assert!(registry.get("test_tool").is_some());
    }

    #[tokio::test]
    async fn test_registry_execute() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        let params = serde_json::json!({
            "name": "test"
        });

        let result = registry.execute("test_tool", &params).await;
        assert!(result.is_ok());
        assert!(result.unwrap().success);
    }

    #[tokio::test]
    async fn test_registry_not_found() {
        let mut registry = ToolRegistry::new();

        let params = serde_json::json!({});
        let result = registry.execute("nonexistent", &params).await;

        assert!(result.is_err());
        matches!(result, Err(ToolError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_registry_categories() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);
        registry.add_to_category("test_cat", "test_tool");

        let categories = registry.categories();
        assert!(categories.contains(&"test_cat"));

        let tools = registry.by_category("test_cat");
        assert_eq!(tools.len(), 1);
    }

    #[tokio::test]
    async fn test_registry_metadata() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        let metadata = registry.metadata("test_tool");
        assert!(metadata.is_some());
        let meta = metadata.unwrap();
        assert_eq!(meta.name, "test_tool");
        assert_eq!(meta.call_count, 0);
    }

    #[tokio::test]
    async fn test_registry_execute_updates_stats() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        let params = serde_json::json!({"name": "test"});
        let _ = registry.execute("test_tool", &params).await;

        let metadata = registry.metadata("test_tool").unwrap();
        assert_eq!(metadata.call_count, 1);
        assert_eq!(metadata.success_count, 1);
    }

    #[tokio::test]
    async fn test_registry_summary() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        let params = serde_json::json!({"name": "test"});
        registry.execute("test_tool", &params).await.unwrap();

        let summary = registry.summary();
        assert_eq!(summary.total_tools, 1);
        assert_eq!(summary.total_calls, 1);
        assert_eq!(summary.success_rate, 1.0);
    }

    #[tokio::test]
    async fn test_tool_list_for_llm() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        let list = registry.tool_list_for_llm();
        assert!(list.contains("test_tool"));
        assert!(list.contains("A test tool"));
    }

    #[tokio::test]
    async fn test_registry_names() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        let names = registry.names();
        assert_eq!(names.len(), 1);
        assert!(names.contains(&"test_tool".to_string()));
    }
}
