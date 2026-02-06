//! Tools - 受控工具集
//!
//! 职责：
//! - 提供安全的文件操作
//! - 提供安全的 Git 操作
//! - 提供安全的 Shell 命令执行
//! - 所有操作都经过验证和日志记录

mod trait_mod;
pub use trait_mod::{Tool, ToolResult, ToolError};

pub mod fs;
pub use fs::FsTool;

pub mod git;
pub use git::GitTool;

pub mod shell;
pub use shell::ShellTool;

/// 工具注册表
#[derive(Debug, Default)]
pub struct ToolRegistry {
    tools: std::collections::HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self::default();

        // 注册内置工具
        registry.register("fs", Arc::new(FsTool::new()));
        registry.register("git", Arc::new(GitTool::new()));
        registry.register("shell", Arc::new(ShellTool::new()));

        registry
    }

    pub fn register(&mut self, name: impl Into<String>, tool: Arc<dyn Tool>) {
        self.tools.insert(name.into(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
}
