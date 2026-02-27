//! Permission Engine — tool execution permission resolution and enforcement.
//!
//! Extracted from `agent_mode.rs` (SEC-S1 God Object refactoring).

use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::debug;

use ndc_core::{AgentError, ToolExecutor};
use ndc_runtime::tools::{
    ToolError, ToolRegistry, extract_confirmation_permission, with_security_overrides,
};

/// 权限规则
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionRule {
    /// 允许
    Allow,
    /// 需要确认
    Ask,
    /// 拒绝
    Deny,
}

/// A permission confirmation request sent from the tool executor to the TUI event loop.
pub struct PermissionRequest {
    /// Human-readable description of the operation being requested.
    pub description: String,
    /// Permission key (e.g. "shell_high_risk", "git_commit") for session/permanent approval.
    pub permission_key: Option<String>,
    /// Send `true` to allow, `false` to deny.
    pub response_tx: oneshot::Sender<bool>,
}

/// REPL Tool Executor - 桥接 Agent Orchestrator 和 Tool Registry
pub struct ReplToolExecutor {
    tool_registry: Arc<ToolRegistry>,
    permissions: HashMap<String, PermissionRule>,
    runtime_working_dir: Arc<Mutex<Option<PathBuf>>>,
    /// Channel to send permission requests to the TUI event loop.
    /// When `None`, falls back to stdin-based confirmation (non-TUI mode).
    permission_tx: Option<mpsc::Sender<PermissionRequest>>,
}

impl ReplToolExecutor {
    pub fn new(
        tool_registry: Arc<ToolRegistry>,
        permissions: HashMap<String, PermissionRule>,
        runtime_working_dir: Arc<Mutex<Option<PathBuf>>>,
    ) -> Self {
        Self {
            tool_registry,
            permissions,
            runtime_working_dir,
            permission_tx: None,
        }
    }

    pub fn with_permission_channel(mut self, tx: mpsc::Sender<PermissionRequest>) -> Self {
        self.permission_tx = Some(tx);
        self
    }

    pub(crate) fn resolve_permission_rule(&self, key: &str) -> PermissionRule {
        self.permissions
            .get(key)
            .cloned()
            .or_else(|| self.permissions.get("*").cloned())
            .unwrap_or(PermissionRule::Ask)
    }

    pub(crate) fn classify_permission(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
    ) -> (String, String) {
        match tool_name {
            "write" | "edit" => (
                "file_write".to_string(),
                format!(
                    "{} {}",
                    tool_name,
                    params
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<unknown>")
                ),
            ),
            "read" | "list" | "grep" | "glob" => (
                "file_read".to_string(),
                format!(
                    "{} {}",
                    tool_name,
                    params
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<unknown>")
                ),
            ),
            "webfetch" | "websearch" => ("network".to_string(), format!("{} request", tool_name)),
            "shell" => (
                "shell_execute".to_string(),
                format!(
                    "shell {} {:?}",
                    params
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("<unknown>"),
                    params
                        .get("args")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default()
                ),
            ),
            "git" => {
                let operation = params
                    .get("operation")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                if operation == "commit" {
                    ("git_commit".to_string(), "git commit".to_string())
                } else {
                    ("git".to_string(), format!("git {}", operation))
                }
            }
            "fs" => {
                let operation = params
                    .get("operation")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let path = params
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<unknown>");
                match operation {
                    "delete" => ("file_delete".to_string(), format!("delete {}", path)),
                    "write" | "create" => {
                        ("file_write".to_string(), format!("{} {}", operation, path))
                    }
                    _ => ("file_read".to_string(), format!("{} {}", operation, path)),
                }
            }
            name if name.starts_with("ndc_task_") => (
                "task_manage".to_string(),
                format!("manage task via {}", name),
            ),
            name if name.starts_with("ndc_memory_") => (
                "task_manage".to_string(),
                format!("query memory via {}", name),
            ),
            _ => ("*".to_string(), format!("tool {}", tool_name)),
        }
    }

    pub(crate) async fn confirm_operation(
        &self,
        description: String,
        permission_key: Option<String>,
    ) -> Result<bool, AgentError> {
        if std::env::var("NDC_AUTO_APPROVE_TOOLS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            return Ok(true);
        }

        // If we have a TUI channel, use it instead of stdin
        if let Some(tx) = &self.permission_tx {
            let (resp_tx, resp_rx) = oneshot::channel();
            tx.send(PermissionRequest {
                description: description.clone(),
                permission_key,
                response_tx: resp_tx,
            })
            .await
            .map_err(|_| {
                AgentError::PermissionDenied(format!("Permission channel closed: {}", description))
            })?;
            return resp_rx.await.map_err(|_| {
                AgentError::PermissionDenied(format!(
                    "Permission response channel dropped: {}",
                    description
                ))
            });
        }

        // Fallback: stdin-based confirmation for non-TUI mode
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(AgentError::PermissionDenied(format!(
                "non_interactive confirmation required: {}; set NDC_AUTO_APPROVE_TOOLS=1 for CI/tests or configure explicit allow policy",
                description
            )));
        }

        tokio::task::spawn_blocking(move || -> Result<bool, String> {
            print!("\n[Permission] {}. Allow? [y/N]: ", description);
            io::stdout().flush().map_err(|e| e.to_string())?;
            let mut line = String::new();
            io::stdin()
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            let answer = line.trim().to_ascii_lowercase();
            Ok(matches!(answer.as_str(), "y" | "yes"))
        })
        .await
        .map_err(|e| AgentError::PermissionDenied(format!("Permission prompt failed: {}", e)))?
        .map_err(AgentError::PermissionDenied)
    }

    fn map_tool_error(err: ToolError) -> AgentError {
        match err {
            ToolError::PermissionDenied(message) => AgentError::PermissionDenied(message),
            other => AgentError::ToolError(format!("Tool execution failed: {}", other)),
        }
    }

    async fn inject_runtime_working_dir(&self, tool_name: &str, params: &mut serde_json::Value) {
        if !matches!(tool_name, "shell" | "fs") {
            return;
        }
        let Some(path) = self.runtime_working_dir.lock().await.clone() else {
            return;
        };
        let Some(obj) = params.as_object_mut() else {
            return;
        };
        if obj.contains_key("working_dir") {
            return;
        }
        obj.insert(
            "working_dir".to_string(),
            serde_json::Value::String(path.to_string_lossy().to_string()),
        );
    }

    async fn execute_tool_with_runtime_confirmation(
        &self,
        tool: Arc<dyn ndc_runtime::tools::Tool>,
        params: &serde_json::Value,
        description: &str,
    ) -> Result<ndc_runtime::tools::ToolResult, AgentError> {
        let mut approved_permissions = std::collections::BTreeSet::<String>::new();

        for _attempt in 0..4 {
            let run = async { tool.execute(params).await };
            let execute_result = if approved_permissions.is_empty() {
                run.await
            } else {
                let overrides = approved_permissions.iter().cloned().collect::<Vec<_>>();
                with_security_overrides(overrides.as_slice(), run).await
            };

            match execute_result {
                Ok(result) => return Ok(result),
                Err(ToolError::PermissionDenied(message)) => {
                    let Some(permission) = extract_confirmation_permission(message.as_str()) else {
                        return Err(AgentError::PermissionDenied(message));
                    };
                    if approved_permissions.contains(permission) {
                        return Err(AgentError::PermissionDenied(message));
                    }

                    let allowed = self
                        .confirm_operation(
                            format!("{} [{}]", description, message),
                            Some(permission.to_string()),
                        )
                        .await?;
                    if !allowed {
                        return Err(AgentError::PermissionDenied(format!(
                            "permission_rejected: {}",
                            message
                        )));
                    }
                    println!("[Permission] approved {} (single tool call)", permission);
                    approved_permissions.insert(permission.to_string());
                }
                Err(other) => return Err(Self::map_tool_error(other)),
            }
        }

        Err(AgentError::PermissionDenied(
            "Permission confirmation loop exceeded retry limit".to_string(),
        ))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for ReplToolExecutor {
    async fn execute_tool(&self, name: &str, arguments: &str) -> Result<String, AgentError> {
        debug!(tool = %name, args = %arguments, "Executing tool via REPL ToolExecutor");

        // 解析参数
        let mut params: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| AgentError::ToolError(format!("Invalid arguments: {}", e)))?;
        self.inject_runtime_working_dir(name, &mut params).await;

        let (permission_key, description) = self.classify_permission(name, &params);
        match self.resolve_permission_rule(&permission_key) {
            PermissionRule::Allow => {}
            PermissionRule::Deny => {
                return Err(AgentError::PermissionDenied(format!(
                    "Permission denied for {} ({})",
                    description, permission_key
                )));
            }
            PermissionRule::Ask => {
                let allowed = self
                    .confirm_operation(description.clone(), Some(permission_key.clone()))
                    .await?;
                if !allowed {
                    return Err(AgentError::PermissionDenied(format!(
                        "User rejected operation: {}",
                        description
                    )));
                }
            }
        }

        // 查找工具
        let tool = self
            .tool_registry
            .get(name)
            .ok_or_else(|| AgentError::ToolError(format!("Tool '{}' not found", name)))?
            .clone();

        // 执行工具 (Tool::execute 只需要一个参数)
        let result = tool.execute(&params).await.map_err(Self::map_tool_error)?;

        if result.success {
            Ok(result.output)
        } else {
            Err(AgentError::ToolError(
                result.error.unwrap_or_else(|| "Unknown error".to_string()),
            ))
        }
    }

    async fn confirm_and_retry_permission(
        &self,
        name: &str,
        arguments: &str,
        permission_message: &str,
    ) -> Result<Option<String>, AgentError> {
        if extract_confirmation_permission(permission_message).is_none() {
            return Ok(None);
        }

        let mut params: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| AgentError::ToolError(format!("Invalid arguments: {}", e)))?;
        self.inject_runtime_working_dir(name, &mut params).await;
        let (_, description) = self.classify_permission(name, &params);
        let tool = self
            .tool_registry
            .get(name)
            .ok_or_else(|| AgentError::ToolError(format!("Tool '{}' not found", name)))?
            .clone();

        let result = self
            .execute_tool_with_runtime_confirmation(tool, &params, description.as_str())
            .await?;
        if result.success {
            Ok(Some(result.output))
        } else {
            Err(AgentError::ToolError(
                result.error.unwrap_or_else(|| "Unknown error".to_string()),
            ))
        }
    }

    fn list_tools(&self) -> Vec<String> {
        self.tool_registry.names()
    }

    fn tool_schemas(&self) -> Vec<serde_json::Value> {
        self.tool_registry
            .all()
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name(),
                        "description": tool.description(),
                        "parameters": tool.schema(),
                    }
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use ndc_runtime::tools::{Tool, ToolMetadata, ToolResult};

    // Serialize env-mutating tests.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[derive(Debug)]
    struct DummyWriteTool;

    #[async_trait]
    impl Tool for DummyWriteTool {
        fn name(&self) -> &str {
            "write"
        }

        fn description(&self) -> &str {
            "dummy write"
        }

        async fn execute(&self, _params: &serde_json::Value) -> Result<ToolResult, ToolError> {
            Ok(ToolResult {
                success: true,
                output: "ok".to_string(),
                error: None,
                metadata: ToolMetadata::default(),
            })
        }
    }

    #[derive(Debug)]
    struct DummyRuntimeDeniedTool;

    #[async_trait]
    impl Tool for DummyRuntimeDeniedTool {
        fn name(&self) -> &str {
            "write"
        }

        fn description(&self) -> &str {
            "dummy denied write"
        }

        async fn execute(&self, _params: &serde_json::Value) -> Result<ToolResult, ToolError> {
            Err(ToolError::PermissionDenied(
                "external_directory requires confirmation".to_string(),
            ))
        }
    }

    #[derive(Debug)]
    struct DummyRuntimeGitCommitTool;

    #[async_trait]
    impl Tool for DummyRuntimeGitCommitTool {
        fn name(&self) -> &str {
            "git"
        }

        fn description(&self) -> &str {
            "dummy git commit gate"
        }

        async fn execute(&self, _params: &serde_json::Value) -> Result<ToolResult, ToolError> {
            ndc_runtime::tools::enforce_git_operation("commit")?;
            Ok(ToolResult {
                success: true,
                output: "commit-ok".to_string(),
                error: None,
                metadata: ToolMetadata::default(),
            })
        }
    }

    #[tokio::test]
    async fn test_permission_deny_blocks_tool_execution() {
        let mut registry = ToolRegistry::new();
        registry.register(DummyWriteTool);
        let registry = Arc::new(registry);

        let mut permissions = HashMap::new();
        permissions.insert("file_write".to_string(), PermissionRule::Deny);
        permissions.insert("*".to_string(), PermissionRule::Allow);

        let executor = ReplToolExecutor::new(
            registry,
            permissions,
            Arc::new(tokio::sync::Mutex::new(None)),
        );
        let result = executor
            .execute_tool("write", r#"{"path":"/tmp/a.txt","content":"x"}"#)
            .await;
        assert!(matches!(result, Err(AgentError::PermissionDenied(_))));
    }

    #[tokio::test]
    async fn test_permission_ask_auto_approve_allows_tool_execution() {
        unsafe {
            std::env::set_var("NDC_AUTO_APPROVE_TOOLS", "1");
        }

        let mut registry = ToolRegistry::new();
        registry.register(DummyWriteTool);
        let registry = Arc::new(registry);

        let mut permissions = HashMap::new();
        permissions.insert("file_write".to_string(), PermissionRule::Ask);
        permissions.insert("*".to_string(), PermissionRule::Allow);

        let executor = ReplToolExecutor::new(
            registry,
            permissions,
            Arc::new(tokio::sync::Mutex::new(None)),
        );
        let result = executor
            .execute_tool("write", r#"{"path":"/tmp/a.txt","content":"x"}"#)
            .await;
        assert!(result.is_ok());

        unsafe {
            std::env::remove_var("NDC_AUTO_APPROVE_TOOLS");
        }
    }

    #[tokio::test]
    async fn test_runtime_permission_denied_maps_to_agent_permission_denied() {
        let mut registry = ToolRegistry::new();
        registry.register(DummyRuntimeDeniedTool);
        let registry = Arc::new(registry);

        let mut permissions = HashMap::new();
        permissions.insert("file_write".to_string(), PermissionRule::Allow);
        permissions.insert("*".to_string(), PermissionRule::Allow);

        let executor = ReplToolExecutor::new(
            registry,
            permissions,
            Arc::new(tokio::sync::Mutex::new(None)),
        );
        let result = executor
            .execute_tool("write", r#"{"path":"/tmp/a.txt","content":"x"}"#)
            .await;
        assert!(matches!(result, Err(AgentError::PermissionDenied(_))));
    }

    #[tokio::test]
    async fn test_runtime_permission_ask_can_auto_confirm_and_retry() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY", "1");
        }
        unsafe {
            std::env::set_var("NDC_SECURITY_GIT_COMMIT_ACTION", "ask");
        }
        unsafe {
            std::env::set_var("NDC_AUTO_APPROVE_TOOLS", "1");
        }

        let mut registry = ToolRegistry::new();
        registry.register(DummyRuntimeGitCommitTool);
        let registry = Arc::new(registry);

        let mut permissions = HashMap::new();
        permissions.insert("git_commit".to_string(), PermissionRule::Allow);
        permissions.insert("*".to_string(), PermissionRule::Allow);

        let executor = ReplToolExecutor::new(
            registry,
            permissions,
            Arc::new(tokio::sync::Mutex::new(None)),
        );
        let initial = executor
            .execute_tool("git", r#"{"operation":"commit"}"#)
            .await;
        let permission_message = match initial {
            Err(AgentError::PermissionDenied(message)) => message,
            other => panic!(
                "expected permission denied on first attempt, got {:?}",
                other
            ),
        };
        assert!(permission_message.starts_with("requires_confirmation permission=git_commit"));

        let retry = executor
            .confirm_and_retry_permission(
                "git",
                r#"{"operation":"commit"}"#,
                permission_message.as_str(),
            )
            .await
            .expect("retry result");
        assert_eq!(retry.as_deref(), Some("commit-ok"));

        unsafe {
            std::env::remove_var("NDC_AUTO_APPROVE_TOOLS");
        }
        unsafe {
            std::env::remove_var("NDC_SECURITY_GIT_COMMIT_ACTION");
        }
        unsafe {
            std::env::remove_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY");
        }
    }

    #[tokio::test]
    async fn test_runtime_permission_retry_non_interactive_returns_denied() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY", "1");
        }
        unsafe {
            std::env::set_var("NDC_SECURITY_GIT_COMMIT_ACTION", "ask");
        }
        unsafe {
            std::env::remove_var("NDC_AUTO_APPROVE_TOOLS");
        }

        let mut registry = ToolRegistry::new();
        registry.register(DummyRuntimeGitCommitTool);
        let registry = Arc::new(registry);

        let mut permissions = HashMap::new();
        permissions.insert("git_commit".to_string(), PermissionRule::Allow);
        permissions.insert("*".to_string(), PermissionRule::Allow);

        let executor = ReplToolExecutor::new(
            registry,
            permissions,
            Arc::new(tokio::sync::Mutex::new(None)),
        );
        let result = executor
            .confirm_and_retry_permission(
                "git",
                r#"{"operation":"commit"}"#,
                "requires_confirmation permission=git_commit risk=high git commit requires confirmation",
            )
            .await;
        assert!(
            matches!(result, Err(AgentError::PermissionDenied(message)) if message.contains("non_interactive confirmation required"))
        );

        unsafe {
            std::env::remove_var("NDC_SECURITY_GIT_COMMIT_ACTION");
        }
        unsafe {
            std::env::remove_var("NDC_SECURITY_PERMISSION_ENFORCE_GATEWAY");
        }
    }

    #[tokio::test]
    async fn test_confirm_operation_via_channel_approved() {
        let (tx, mut rx) = mpsc::channel::<PermissionRequest>(4);
        let mut registry = ToolRegistry::new();
        registry.register(DummyWriteTool);
        let registry = Arc::new(registry);

        let mut permissions = HashMap::new();
        permissions.insert("*".to_string(), PermissionRule::Ask);

        let executor = ReplToolExecutor::new(
            registry,
            permissions,
            Arc::new(tokio::sync::Mutex::new(None)),
        )
        .with_permission_channel(tx);

        // Spawn a task to respond to the permission request
        let responder = tokio::spawn(async move {
            let req = rx.recv().await.expect("should receive permission request");
            assert!(req.description.contains("write"));
            req.response_tx.send(true).expect("send response");
        });

        let result = executor
            .confirm_operation("test write operation".to_string(), None)
            .await
            .expect("should not error");
        assert!(result);
        responder.await.unwrap();
    }

    #[tokio::test]
    async fn test_confirm_operation_via_channel_denied() {
        let (tx, mut rx) = mpsc::channel::<PermissionRequest>(4);
        let mut registry = ToolRegistry::new();
        registry.register(DummyWriteTool);
        let registry = Arc::new(registry);

        let mut permissions = HashMap::new();
        permissions.insert("*".to_string(), PermissionRule::Ask);

        let executor = ReplToolExecutor::new(
            registry,
            permissions,
            Arc::new(tokio::sync::Mutex::new(None)),
        )
        .with_permission_channel(tx);

        let responder = tokio::spawn(async move {
            let req = rx.recv().await.expect("should receive permission request");
            req.response_tx.send(false).expect("send response");
        });

        let result = executor
            .confirm_operation("test write operation".to_string(), None)
            .await
            .expect("should not error");
        assert!(!result);
        responder.await.unwrap();
    }

    #[test]
    fn test_resolve_permission_wildcard_fallback() {
        let mut permissions = HashMap::new();
        permissions.insert("file_read".to_string(), PermissionRule::Allow);
        permissions.insert("*".to_string(), PermissionRule::Deny);

        let executor = ReplToolExecutor::new(
            Arc::new(ToolRegistry::new()),
            permissions,
            Arc::new(tokio::sync::Mutex::new(None)),
        );

        // Exact match
        assert_eq!(
            executor.resolve_permission_rule("file_read"),
            PermissionRule::Allow
        );
        // Wildcard fallback
        assert_eq!(
            executor.resolve_permission_rule("shell_execute"),
            PermissionRule::Deny
        );
        assert_eq!(
            executor.resolve_permission_rule("network"),
            PermissionRule::Deny
        );
    }

    #[test]
    fn test_resolve_permission_no_wildcard_defaults_to_ask() {
        let mut permissions = HashMap::new();
        permissions.insert("file_read".to_string(), PermissionRule::Allow);
        // No "*" entry

        let executor = ReplToolExecutor::new(
            Arc::new(ToolRegistry::new()),
            permissions,
            Arc::new(tokio::sync::Mutex::new(None)),
        );

        // Known key
        assert_eq!(
            executor.resolve_permission_rule("file_read"),
            PermissionRule::Allow
        );
        // Unknown key with no wildcard → defaults to Ask
        assert_eq!(
            executor.resolve_permission_rule("shell_execute"),
            PermissionRule::Ask
        );
    }

    #[test]
    fn test_classify_unknown_tool_uses_wildcard_key() {
        let executor = ReplToolExecutor::new(
            Arc::new(ToolRegistry::new()),
            HashMap::new(),
            Arc::new(tokio::sync::Mutex::new(None)),
        );

        let (key, desc) = executor.classify_permission("some_custom_tool", &serde_json::json!({}));
        assert_eq!(key, "*");
        assert!(desc.contains("some_custom_tool"));
    }

    #[test]
    fn test_classify_git_commit_vs_other_operation() {
        let executor = ReplToolExecutor::new(
            Arc::new(ToolRegistry::new()),
            HashMap::new(),
            Arc::new(tokio::sync::Mutex::new(None)),
        );

        // git commit → "git_commit"
        let (key, desc) =
            executor.classify_permission("git", &serde_json::json!({"operation": "commit"}));
        assert_eq!(key, "git_commit");
        assert_eq!(desc, "git commit");

        // git push → "git"
        let (key, desc) =
            executor.classify_permission("git", &serde_json::json!({"operation": "push"}));
        assert_eq!(key, "git");
        assert_eq!(desc, "git push");
    }
}
