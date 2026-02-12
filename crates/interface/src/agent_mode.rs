//! Agent Mode - AI Agent REPL Integration
//!
//! 职责:
//! - REPL 的 Agent 交互模式
//! - /agent 命令处理
//! - 流式响应显示
//! - 权限确认 UI
//!
//! 设计理念 (来自 NDC_AGENT_INTEGRATION_PLAN.md):
//! - 使用 OpenCode 的流式响应模式
//! - 使用 OpenCode 的权限确认模式
//! - 增强内置 NDC 工程能力
//! - 集成 NDC 反馈循环验证

use std::sync::Arc;
use std::path::PathBuf;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tracing::{info, debug};

use ndc_core::{
    AgentOrchestrator, AgentConfig, AgentRequest, AgentResponse,
    ToolExecutor, AgentError, TaskVerifier, LlmProvider,
    AgentRole, TaskId, TaskStorage, ProviderType, ProviderConfig,
};
use ndc_runtime::{Executor, tools::ToolRegistry};

/// Get API key from environment variable with NDC_ prefix
fn get_api_key(provider: &str) -> String {
    let env_var = format!("NDC_{}_API_KEY", provider.to_uppercase());
    std::env::var(&env_var).ok()
        .or_else(|| std::env::var("NDC_LLM_API_KEY").ok())
        .unwrap_or_default()
}

/// Get organization/group_id from environment variable
fn get_organization(provider: &str) -> String {
    let env_var = format!("NDC_{}_GROUP_ID", provider.to_uppercase());
    std::env::var(&env_var).ok()
        .unwrap_or_default()
}

/// Create provider configuration based on provider name
fn create_provider_config(provider_name: &str, model: &str) -> ProviderConfig {
    let api_key = get_api_key(provider_name);
    let organization = get_organization(provider_name);
    let provider_type: ProviderType = provider_name.to_string().into();

    let (base_url, models) = match provider_type {
        ProviderType::OpenAi => (
            None,
            vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string(), "gpt-4".to_string()],
        ),
        ProviderType::Anthropic => (
            Some("https://api.anthropic.com/v1".to_string()),
            vec!["claude-sonnet-4-5-20250929".to_string(), "claude-3-5-sonnet".to_string()],
        ),
        ProviderType::MiniMax => (
            Some("https://api.minimax.chat/v1".to_string()),
            vec!["m2.1-0107".to_string(), "abab6.5s-chat".to_string()],
        ),
        ProviderType::OpenRouter => (
            Some("https://openrouter.ai/api/v1".to_string()),
            vec!["anthropic/claude-3.5-sonnet".to_string(), "openai/gpt-4o".to_string()],
        ),
        ProviderType::Ollama => (
            Some("http://localhost:11434".to_string()),
            vec!["llama3.2".to_string(), "llama3".to_string(), "qwen2.5".to_string()],
        ),
        _ => (
            None,
            vec![model.to_string()],
        ),
    };

    ProviderConfig {
        name: provider_name.to_string(),
        provider_type,
        api_key,
        base_url,
        organization: if organization.is_empty() { None } else { Some(organization) },
        default_model: model.to_string(),
        models,
        timeout_ms: 60000,
        max_retries: 3,
    }
}

/// 内存任务存储 - 用于 Agent 验证
struct MemoryTaskStorage {
    tasks: Arc<Mutex<HashMap<String, ndc_core::Task>>>,
}

impl MemoryTaskStorage {
    fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl TaskStorage for MemoryTaskStorage {
    async fn get_task(&self, id: &TaskId) -> Result<Option<ndc_core::Task>, Box<dyn std::error::Error + Send + Sync>> {
        let tasks = self.tasks.lock().await;
        Ok(tasks.get(&id.to_string()).cloned())
    }
}

/// Agent REPL 模式配置
#[derive(Debug, Clone)]
pub struct AgentModeConfig {
    /// Agent 名称
    pub agent_name: String,

    /// Agent 描述
    pub description: String,

    /// LLM Provider 名称
    pub provider: String,

    /// 模型名称
    pub model: String,

    /// 温度
    pub temperature: f32,

    /// 最大工具调用次数
    pub max_tool_calls: usize,

    /// 是否启用流式响应
    pub enable_streaming: bool,

    /// 是否自动验证
    pub auto_verify: bool,

    /// 权限规则: 操作 -> allow/ask/deny
    pub permissions: HashMap<String, PermissionRule>,
}

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

impl Default for AgentModeConfig {
    fn default() -> Self {
        let mut permissions = HashMap::new();
        // 默认权限规则
        permissions.insert("*".to_string(), PermissionRule::Allow);
        permissions.insert("file_write".to_string(), PermissionRule::Ask);
        permissions.insert("git_commit".to_string(), PermissionRule::Ask);
        permissions.insert("file_delete".to_string(), PermissionRule::Ask);

        Self {
            agent_name: "build".to_string(),
            description: "NDC default agent with engineering capabilities".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            temperature: 0.1,
            max_tool_calls: 50,
            enable_streaming: true,
            auto_verify: true,
            permissions,
        }
    }
}

/// Agent REPL 模式状态
#[derive(Debug, Clone)]
pub struct AgentModeState {
    /// 是否启用
    pub enabled: bool,

    /// 当前配置
    pub config: AgentModeConfig,

    /// Agent 会话 ID
    pub session_id: Option<String>,

    /// 活跃任务 ID
    pub active_task_id: Option<TaskId>,

    /// 工作目录
    pub working_dir: Option<PathBuf>,
}

impl Default for AgentModeState {
    fn default() -> Self {
        Self {
            enabled: false,
            config: AgentModeConfig::default(),
            session_id: None,
            active_task_id: None,
            working_dir: None,
        }
    }
}

/// Agent REPL 模式管理器
pub struct AgentModeManager {
    /// 状态
    state: Arc<Mutex<AgentModeState>>,

    /// Orchestrator (可选，仅当启用时创建)
    orchestrator: Arc<Mutex<Option<AgentOrchestrator>>>,

    /// Runtime Executor (保留供未来使用)
    _executor: Arc<Executor>,

    /// Tool Registry
    tool_registry: Arc<ToolRegistry>,
}

impl AgentModeManager {
    /// 创建新的 Agent Mode Manager
    pub fn new(executor: Arc<Executor>, tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            state: Arc::new(Mutex::new(AgentModeState::default())),
            orchestrator: Arc::new(Mutex::new(None)),
            _executor: executor,
            tool_registry,
        }
    }

    /// 启用 Agent 模式
    pub async fn enable(&self, config: AgentModeConfig) -> Result<(), AgentError> {
        let mut state = self.state.lock().await;
        state.enabled = true;
        state.config = config.clone();
        state.session_id = Some(format!("agent-{}", ulid::Ulid::new()));

        // 创建 Agent Orchestrator
        let tool_executor = Arc::new(ReplToolExecutor::new(self.tool_registry.clone()));
        let provider = self.create_provider(&config.provider)?;

        // 创建简单的内存存储用于 TaskVerifier
        let storage = Arc::new(MemoryTaskStorage::new());
        let verifier = Arc::new(TaskVerifier::new(storage));

        let agent_config = AgentConfig {
            max_tool_calls: config.max_tool_calls,
            enable_streaming: config.enable_streaming,
            auto_verify: config.auto_verify,
            ..Default::default()
        };

        let orchestrator = AgentOrchestrator::new(
            provider,
            tool_executor,
            verifier,
            agent_config,
        );

        let mut orch = self.orchestrator.lock().await;
        *orch = Some(orchestrator);

        info!(agent = %config.agent_name, "Agent mode enabled");
        Ok(())
    }

    /// 禁用 Agent 模式
    pub async fn disable(&self) {
        let mut state = self.state.lock().await;
        state.enabled = false;
        state.session_id = None;
        state.active_task_id = None;

        let mut orch = self.orchestrator.lock().await;
        *orch = None;

        info!("Agent mode disabled");
    }

    /// 检查是否启用
    pub async fn is_enabled(&self) -> bool {
        let state = self.state.lock().await;
        state.enabled
    }

    /// 处理用户输入 (非流式)
    pub async fn process_input(&self, input: &str) -> Result<AgentResponse, AgentError> {
        let state = self.state.lock().await;

        if !state.enabled {
            return Err(AgentError::InvalidRequest("Agent mode is not enabled".to_string()));
        }

        let session_id = state.session_id.clone();
        let working_dir = state.working_dir.clone();
        let active_task_id = state.active_task_id;

        drop(state);

        let orch = self.orchestrator.lock().await;
        let orchestrator = orch.as_ref()
            .ok_or_else(|| AgentError::InvalidRequest("Orchestrator not initialized".to_string()))?;

        let request = AgentRequest {
            user_input: input.to_string(),
            session_id,
            working_dir,
            role: Some(AgentRole::Implementer),
            active_task_id,
        };

        orchestrator.process(request).await
    }

    /// 设置活跃任务
    pub async fn set_active_task(&self, task_id: TaskId) {
        let mut state = self.state.lock().await;
        state.active_task_id = Some(task_id);
    }

    /// 获取状态信息
    pub async fn status(&self) -> AgentModeStatus {
        let state = self.state.lock().await;
        AgentModeStatus {
            enabled: state.enabled,
            agent_name: state.config.agent_name.clone(),
            provider: state.config.provider.clone(),
            model: state.config.model.clone(),
            session_id: state.session_id.clone(),
            active_task_id: state.active_task_id,
        }
    }

    /// 切换 LLM Provider
    pub async fn switch_provider(&self, provider_name: &str, model: Option<&str>) -> Result<(), AgentError> {
        let mut state = self.state.lock().await;

        // 检查是否启用
        let was_enabled = state.enabled;

        // 更新配置
        state.config.provider = provider_name.to_string();
        let new_model = if let Some(m) = model {
            m.to_string()
        } else {
            // 设置默认模型
            match provider_name {
                "openai" => "gpt-4o".to_string(),
                "anthropic" => "claude-sonnet-4-5-20250929".to_string(),
                "minimax" => "m2.1-0107".to_string(),
                "openrouter" => "anthropic/claude-3.5-sonnet".to_string(),
                "ollama" => "llama3.2".to_string(),
                _ => provider_name.to_string(),
            }
        };
        state.config.model = new_model.clone();

        // 克隆更新后的配置
        let config = state.config.clone();

        drop(state);

        // 重新创建 orchestrator (如果之前已启用)
        if was_enabled {
            self.disable().await;
            self.enable(config).await?;
        }

        info!(provider = %provider_name, model = %new_model, "Provider switched");
        Ok(())
    }

    /// 创建 LLM Provider
    fn create_provider(&self, provider_name: &str) -> Result<Arc<dyn LlmProvider>, AgentError> {
        use ndc_core::llm::provider::{SimpleTokenCounter, OpenAiProvider, AnthropicProvider, MiniMaxProvider, OpenRouterProvider, TokenCounter};

        // 根据 provider 名称创建相应的 Provider
        let provider_type: ProviderType = provider_name.to_string().into();
        let token_counter: Arc<dyn TokenCounter> = Arc::new(SimpleTokenCounter::new());

        match provider_type {
            ProviderType::OpenAi => {
                let config = create_provider_config(provider_name, "gpt-4o");
                let provider = OpenAiProvider::new(config, token_counter);
                Ok(Arc::new(provider))
            }
            ProviderType::Anthropic => {
                let config = create_provider_config(provider_name, "claude-sonnet-4-5-20250929");
                let provider = AnthropicProvider::new(config, token_counter);
                Ok(Arc::new(provider))
            }
            ProviderType::MiniMax => {
                let config = create_provider_config(provider_name, "m2.1-0107");
                let provider = MiniMaxProvider::new(config, token_counter);
                Ok(Arc::new(provider))
            }
            ProviderType::OpenRouter => {
                let config = create_provider_config(provider_name, "anthropic/claude-3.5-sonnet");
                let provider = OpenRouterProvider::new(config, token_counter);
                Ok(Arc::new(provider))
            }
            ProviderType::Ollama => {
                let config = create_provider_config(provider_name, "llama3.2");
                let provider = OpenAiProvider::new(config, token_counter);
                Ok(Arc::new(provider))
            }
            _ => Err(AgentError::InvalidRequest(
                format!("Provider '{}' is not supported. Supported: openai, anthropic, minimax, openrouter, ollama", provider_name)
            ))
        }
    }
}

/// Agent 模式状态信息
#[derive(Debug, Clone)]
pub struct AgentModeStatus {
    pub enabled: bool,
    pub agent_name: String,
    pub provider: String,
    pub model: String,
    pub session_id: Option<String>,
    pub active_task_id: Option<TaskId>,
}

/// REPL Tool Executor - 桥接 Agent Orchestrator 和 Tool Registry
pub struct ReplToolExecutor {
    tool_registry: Arc<ToolRegistry>,
}

impl ReplToolExecutor {
    pub fn new(tool_registry: Arc<ToolRegistry>) -> Self {
        Self { tool_registry }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for ReplToolExecutor {
    async fn execute_tool(&self, name: &str, arguments: &str) -> Result<String, AgentError> {
        debug!(tool = %name, args = %arguments, "Executing tool via REPL ToolExecutor");

        // 查找工具
        let tool = self.tool_registry.get(name)
            .ok_or_else(|| AgentError::ToolError(format!("Tool '{}' not found", name)))?;

        // 解析参数
        let params: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| AgentError::ToolError(format!("Invalid arguments: {}", e)))?;

        // 执行工具 (Tool::execute 只需要一个参数)
        let result = tool.execute(&params).await
            .map_err(|e| AgentError::ToolError(format!("Tool execution failed: {}", e)))?;

        if result.success {
            Ok(result.output)
        } else {
            Err(AgentError::ToolError(result.error.unwrap_or_else(|| "Unknown error".to_string())))
        }
    }

    fn list_tools(&self) -> Vec<String> {
        self.tool_registry.names()
    }
}

/// 显示 Agent 状态
pub fn show_agent_status(status: AgentModeStatus) {
    println!("\n┌─────────────────────────────────────────────────────────────────┐");
    println!("│  AI Agent Mode Status                                            │");
    println!("├─────────────────────────────────────────────────────────────────┤");
    println!("│  Status: {}                                                     │",
        if status.enabled { "🟢 Enabled" } else { "⚪ Disabled" });
    if status.enabled {
        println!("│  Agent: {}                                                      │", status.agent_name);
        println!("│  Provider: {} @ {}                                               │", status.provider, status.model);
        if let Some(sid) = &status.session_id {
            println!("│  Session: {}                                                   │", sid);
        }
        if let Some(tid) = &status.active_task_id {
            println!("│  Active Task: {}                                                │", tid);
        }
    }
    println!("└─────────────────────────────────────────────────────────────────┘\n");
}

/// 处理 /agent 命令
pub async fn handle_agent_command(
    input: &str,
    manager: &AgentModeManager,
) -> Result<bool, AgentError> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let cmd = parts.get(1).unwrap_or(&"help");

    match *cmd {
        "on" | "enable" => {
            let config = AgentModeConfig::default();
            manager.enable(config).await?;
            println!("\n✅ Agent Mode Enabled\n");
            show_agent_status(manager.status().await);
            println!("💡 Type your message to interact with the AI agent.");
            println!("   Use '/agent off' to disable.\n");
            Ok(true)
        }
        "off" | "disable" => {
            manager.disable().await;
            println!("\n🔴 Agent Mode Disabled\n");
            Ok(true)
        }
        "status" => {
            show_agent_status(manager.status().await);
            Ok(true)
        }
        "help" => {
            show_agent_help();
            Ok(true)
        }
        _ => {
            println!("Unknown agent command: {}", cmd);
            show_agent_help();
            Ok(true)
        }
    }
}

/// 显示 Agent 命令帮助
fn show_agent_help() {
    println!("\n┌─────────────────────────────────────────────────────────────────┐");
    println!("│  Agent Mode Commands                                             │");
    println!("├─────────────────────────────────────────────────────────────────┤");
    println!("│  /agent on       Enable AI agent mode                            │");
    println!("│  /agent off      Disable AI agent mode                           │");
    println!("│  /agent status   Show agent status                               │");
    println!("│  /agent help     Show this help message                          │");
    println!("├─────────────────────────────────────────────────────────────────┤");
    println!("│  When agent mode is enabled:                                      │");
    println!("│  - Your messages will be processed by the AI agent               │");
    println!("│  - The agent can use tools to complete tasks                     │");
    println!("│  - Use /agent off to return to normal REPL mode                  │");
    println!("└─────────────────────────────────────────────────────────────────┘\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_mode_config_default() {
        let config = AgentModeConfig::default();
        assert_eq!(config.agent_name, "build");
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "gpt-4o");
        assert!(config.enable_streaming);
        assert!(config.auto_verify);
    }

    #[test]
    fn test_permission_rule() {
        let allow = PermissionRule::Allow;
        let ask = PermissionRule::Ask;
        let deny = PermissionRule::Deny;

        assert_eq!(allow, PermissionRule::Allow);
        assert_eq!(ask, PermissionRule::Ask);
        assert_eq!(deny, PermissionRule::Deny);
    }

    #[test]
    fn test_agent_mode_state_default() {
        let state = AgentModeState::default();
        assert!(!state.enabled);
        assert_eq!(state.config.agent_name, "build");
        assert!(state.session_id.is_none());
        assert!(state.active_task_id.is_none());
    }

    #[tokio::test]
    async fn test_agent_mode_manager_create() {
        // This is a basic smoke test - full integration tests require
        // more setup with actual Executor and ToolRegistry
        let config = AgentModeConfig::default();
        assert_eq!(config.agent_name, "build");
    }
}
