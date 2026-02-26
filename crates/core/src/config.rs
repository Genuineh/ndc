//! NDC 配置系统 (OpenCode 风格)
//!
//! 特点:
//! - 配置分层: 全局 > 用户 > 项目
//! - 环境变量支持: NDC_* 前缀
//! - 敏感信息注入: 通过环境变量
//! - 统一类型定义 (使用 llm/provider 中的 ProviderType)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use thiserror::Error;

// Re-export from llm/provider
pub use crate::llm::provider::ProviderConfig;

/// 配置错误
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Config file not found: {0}")]
    NotFound(PathBuf),

    #[error("Config parse error: {0}")]
    ParseError(String),

    #[error("Config validation error: {0}")]
    ValidationError(String),

    #[error("Environment variable not set: {0}")]
    EnvVarNotSet(String),
}

// ============================================================================
// YAML 配置类型 (用于配置文件反序列化)
// ============================================================================

/// NDC 主配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "YamlNdcConfig", into = "YamlNdcConfig")]
pub struct NdcConfig {
    pub llm: Option<YamlLlmConfig>,
    pub repl: Option<YamlReplConfig>,
    pub runtime: Option<YamlRuntimeConfig>,
    pub storage: Option<YamlStorageConfig>,
    #[serde(default)]
    pub agents: Vec<YamlAgentProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct YamlNdcConfig {
    pub llm: Option<YamlLlmConfig>,
    pub repl: Option<YamlReplConfig>,
    pub runtime: Option<YamlRuntimeConfig>,
    pub storage: Option<YamlStorageConfig>,
    pub agents: Vec<YamlAgentProfile>,
}

impl From<YamlNdcConfig> for NdcConfig {
    fn from(yaml: YamlNdcConfig) -> Self {
        Self {
            llm: yaml.llm,
            repl: yaml.repl,
            runtime: yaml.runtime,
            storage: yaml.storage,
            agents: yaml.agents,
        }
    }
}

impl From<NdcConfig> for YamlNdcConfig {
    fn from(config: NdcConfig) -> Self {
        Self {
            llm: config.llm,
            repl: config.repl,
            runtime: config.runtime,
            storage: config.storage,
            agents: config.agents,
        }
    }
}

impl Default for NdcConfig {
    fn default() -> Self {
        Self {
            llm: None,
            repl: Some(YamlReplConfig::default()),
            runtime: Some(YamlRuntimeConfig::default()),
            storage: Some(YamlStorageConfig::default()),
            agents: Vec::new(),
        }
    }
}

/// LLM 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlLlmConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub organization: Option<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub providers: HashMap<String, YamlProviderConfig>,
}

fn default_true() -> bool {
    true
}
fn default_provider() -> String {
    "openai".to_string()
}
fn default_model() -> String {
    "gpt-4o".to_string()
}
fn default_temperature() -> f32 {
    0.1
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_timeout() -> u64 {
    60
}

impl Default for YamlLlmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: default_provider(),
            model: default_model(),
            base_url: None,
            api_key: None,
            organization: None,
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            timeout: default_timeout(),
            providers: HashMap::new(),
        }
    }
}

/// Provider 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlProviderConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub organization: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub timeout: Option<u64>,
    pub capabilities: Option<Vec<String>>,
}

/// REPL 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlReplConfig {
    #[serde(default = "default_prompt")]
    pub prompt: String,
    pub history_file: Option<PathBuf>,
    #[serde(default = "default_max_history")]
    pub max_history: usize,
    #[serde(default = "default_true")]
    pub show_thought: bool,
    #[serde(default = "default_true")]
    pub auto_create_task: bool,
    #[serde(default = "default_session_timeout")]
    pub session_timeout: u64,
    #[serde(default = "default_true")]
    pub fallback_to_regex: bool,
    #[serde(default = "default_confirmation")]
    pub confirmation_mode: bool,
}

fn default_prompt() -> String {
    "ndc> ".to_string()
}
fn default_max_history() -> usize {
    1000
}
fn default_session_timeout() -> u64 {
    3600
}
fn default_confirmation() -> bool {
    true
}

impl Default for YamlReplConfig {
    fn default() -> Self {
        Self {
            prompt: default_prompt(),
            history_file: None,
            max_history: default_max_history(),
            show_thought: true,
            auto_create_task: true,
            session_timeout: default_session_timeout(),
            fallback_to_regex: true,
            confirmation_mode: true,
        }
    }
}

/// Runtime 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlRuntimeConfig {
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_tasks: usize,
    #[serde(default = "default_execution_timeout")]
    pub execution_timeout: u64,
    pub working_dir: Option<PathBuf>,
    pub quality_gates: Option<Vec<String>>,
    /// Discovery failure strategy: "degrade" (default) or "block"
    #[serde(default = "default_discovery_failure_mode")]
    pub discovery_failure_mode: String,
}

fn default_max_concurrent() -> usize {
    4
}
fn default_execution_timeout() -> u64 {
    300
}
fn default_discovery_failure_mode() -> String {
    "degrade".to_string()
}

impl Default for YamlRuntimeConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: default_max_concurrent(),
            execution_timeout: default_execution_timeout(),
            working_dir: None,
            quality_gates: None,
            discovery_failure_mode: default_discovery_failure_mode(),
        }
    }
}

/// Storage 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlStorageConfig {
    #[serde(default = "default_storage_type")]
    pub storage_type: String,
    pub db_path: Option<PathBuf>,
    #[serde(default)]
    pub in_memory: bool,
}

fn default_storage_type() -> String {
    "memory".to_string()
}

impl Default for YamlStorageConfig {
    fn default() -> Self {
        Self {
            storage_type: default_storage_type(),
            db_path: None,
            in_memory: true,
        }
    }
}

/// Agent Profile 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlAgentProfile {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub max_tool_calls: Option<usize>,
    #[serde(default = "default_true")]
    pub enable_streaming: bool,
    #[serde(default = "default_true")]
    pub auto_verify: bool,
    pub tool_permissions: Option<YamlToolPermissions>,
    pub task_types: Option<Vec<String>>,
    pub priority: Option<i32>,
}

impl Default for YamlAgentProfile {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            display_name: None,
            description: None,
            provider: None,
            model: None,
            temperature: None,
            max_tokens: None,
            max_tool_calls: None,
            enable_streaming: true,
            auto_verify: true,
            tool_permissions: None,
            task_types: None,
            priority: None,
        }
    }
}

/// Tool permissions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct YamlToolPermissions {
    #[serde(default = "default_permission_rule")]
    pub default: String,
    #[serde(default)]
    pub tools: HashMap<String, String>,
}

fn default_permission_rule() -> String {
    "ask".to_string()
}

// ============================================================================
// OpenCode 风格配置加载器
// ============================================================================

/// 配置分层枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigLayer {
    Global,  // /etc/ndc/
    User,    // ~/.config/ndc/
    Project, // ./.ndc/
}

impl ConfigLayer {
    pub fn path(&self) -> PathBuf {
        match self {
            ConfigLayer::Global => PathBuf::from("/etc/ndc"),
            ConfigLayer::User => {
                let home = env::var("HOME")
                    .or_else(|_| env::var("USERPROFILE"))
                    .unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".config/ndc")
            }
            ConfigLayer::Project => PathBuf::from(".ndc"),
        }
    }
}

/// NDC 配置加载器
#[derive(Debug, Clone)]
pub struct NdcConfigLoader {
    layers: Vec<ConfigLayer>,
    config: NdcConfig,
}

impl NdcConfigLoader {
    pub fn new() -> Self {
        Self {
            layers: vec![ConfigLayer::Global, ConfigLayer::User, ConfigLayer::Project],
            config: NdcConfig::default(),
        }
    }

    pub fn with_layers(layers: Vec<ConfigLayer>) -> Self {
        Self {
            layers,
            config: NdcConfig::default(),
        }
    }

    pub fn load(&mut self) -> Result<&NdcConfig, ConfigError> {
        // Clone layers to avoid borrow conflicts
        let layers = self.layers.clone();
        for layer in layers {
            let path = layer.path().join("config.yaml");
            if path.exists() {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| ConfigError::ParseError(e.to_string()))?;
                let config: NdcConfig = serde_yaml::from_str(&content)
                    .map_err(|e| ConfigError::ParseError(e.to_string()))?;
                self.merge(config);
            }
        }
        self.apply_env_overrides();
        Ok(&self.config)
    }

    fn merge(&mut self, other: NdcConfig) {
        if let Some(llm) = other.llm {
            self.config.llm = Some(llm);
        }
        if let Some(repl) = other.repl {
            self.config.repl = Some(repl);
        }
        if let Some(runtime) = other.runtime {
            self.config.runtime = Some(runtime);
        }
        if let Some(storage) = other.storage {
            self.config.storage = Some(storage);
        }
        for agent in other.agents {
            self.config.agents.retain(|a| a.name != agent.name);
            self.config.agents.push(agent);
        }
    }

    fn apply_env_overrides(&mut self) {
        // LLM 配置
        if env::var("NDC_LLM_PROVIDER").is_ok()
            || env::var("NDC_LLM_MODEL").is_ok()
            || env::var("NDC_LLM_API_KEY").is_ok()
            || env::var("NDC_LLM_BASE_URL").is_ok()
            || env::var("NDC_ORGANIZATION").is_ok()
        {
            let llm = self.config.llm.get_or_insert_with(YamlLlmConfig::default);

            if let Ok(v) = env::var("NDC_LLM_PROVIDER") {
                llm.provider = v;
            }
            if let Ok(v) = env::var("NDC_LLM_MODEL") {
                llm.model = v;
            }
            if let Ok(v) = env::var("NDC_LLM_API_KEY") {
                llm.api_key = Some(v);
            }
            if let Ok(v) = env::var("NDC_LLM_BASE_URL") {
                llm.base_url = Some(v);
            }
            if let Ok(v) = env::var("NDC_ORGANIZATION") {
                llm.organization = Some(v);
            }
        }

        // REPL 配置
        if env::var("NDC_REPL_CONFIRMATION").is_ok() {
            let repl = self.config.repl.get_or_insert_with(YamlReplConfig::default);
            if let Ok(v) = env::var("NDC_REPL_CONFIRMATION") {
                repl.confirmation_mode = v != "false";
            }
        }

        // Runtime 配置
        if let Ok(v) = env::var("NDC_MAX_CONCURRENT_TASKS")
            && let Ok(n) = v.parse() {
                let runtime = self
                    .config
                    .runtime
                    .get_or_insert_with(YamlRuntimeConfig::default);
                runtime.max_concurrent_tasks = n;
            }
        if let Ok(v) = env::var("NDC_DISCOVERY_FAILURE_MODE") {
            let runtime = self
                .config
                .runtime
                .get_or_insert_with(YamlRuntimeConfig::default);
            runtime.discovery_failure_mode = v;
        }
    }

    pub fn config(&self) -> &NdcConfig {
        &self.config
    }

    pub fn create_provider_config(&self) -> Option<ProviderConfig> {
        let llm = self.config.llm.as_ref()?;
        let provider_type = llm.provider.clone().to_lowercase().into();

        Some(ProviderConfig {
            name: llm.provider.clone(),
            provider_type,
            api_key: llm
                .api_key
                .as_deref()
                .and_then(parse_env_ref)
                .or_else(|| env::var("NDC_LLM_API_KEY").ok())
                .unwrap_or_default(),
            base_url: llm
                .base_url
                .as_deref()
                .and_then(parse_env_ref)
                .or_else(|| env::var("NDC_LLM_BASE_URL").ok()),
            organization: llm
                .organization
                .as_deref()
                .and_then(parse_env_ref)
                .or_else(|| env::var("NDC_ORGANIZATION").ok()),
            default_model: llm.model.clone(),
            models: Vec::new(),
            timeout_ms: llm.timeout * 1000,
            max_retries: 3,
        })
    }
}

impl Default for NdcConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析 env:// 前缀
fn parse_env_ref(value: &str) -> Option<String> {
    if let Some(env_var) = value.strip_prefix("env://") {
        env::var(env_var).ok()
    } else {
        Some(value.to_string())
    }
}

// ============================================================================
// Agent Configuration System
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolPermissions {
    #[serde(default)]
    pub default: PermissionRule,
    #[serde(default)]
    pub tools: HashMap<String, PermissionRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum PermissionRule {
    Allow,
    #[default]
    Ask,
    Deny,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub provider: String,
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_max_tool_calls")]
    pub max_tool_calls: usize,
    #[serde(default = "default_true")]
    pub enable_streaming: bool,
    #[serde(default = "default_true")]
    pub auto_verify: bool,
    #[serde(default)]
    pub tool_permissions: ToolPermissions,
    #[serde(default)]
    pub task_types: Vec<String>,
    #[serde(default = "default_priority")]
    pub priority: i32,
}

fn default_max_tool_calls() -> usize {
    50
}
fn default_priority() -> i32 {
    0
}

impl Default for AgentProfile {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            display_name: "Default Agent".to_string(),
            description: "General purpose agent".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            temperature: 0.1,
            max_tokens: 4096,
            max_tool_calls: 50,
            enable_streaming: true,
            auto_verify: true,
            tool_permissions: ToolPermissions::default(),
            task_types: vec!["*".to_string()],
            priority: 0,
        }
    }
}

pub struct PredefinedProfiles;

impl PredefinedProfiles {
    pub fn base() -> AgentProfile {
        AgentProfile::default()
    }

    pub fn implementer() -> AgentProfile {
        AgentProfile {
            name: "implementer".to_string(),
            display_name: "Code Implementer".to_string(),
            description: "Specialized for implementing features".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            temperature: 0.1,
            max_tokens: 8192,
            max_tool_calls: 100,
            enable_streaming: true,
            auto_verify: true,
            tool_permissions: ToolPermissions::default(),
            task_types: vec!["implementation".to_string(), "bugfix".to_string()],
            priority: 10,
        }
    }

    pub fn verifier() -> AgentProfile {
        AgentProfile {
            name: "verifier".to_string(),
            display_name: "Code Verifier".to_string(),
            description: "Specialized for verifying code".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            temperature: 0.0,
            max_tokens: 4096,
            max_tool_calls: 30,
            enable_streaming: false,
            auto_verify: false,
            tool_permissions: ToolPermissions::default(),
            task_types: vec!["verification".to_string()],
            priority: 5,
        }
    }

    pub fn all() -> Vec<AgentProfile> {
        vec![Self::base(), Self::implementer(), Self::verifier()]
    }
}

#[derive(Debug, Clone)]
pub struct AgentRoleSelector {
    profiles: Vec<AgentProfile>,
    default_profile: String,
}

impl AgentRoleSelector {
    pub fn new() -> Self {
        Self {
            profiles: PredefinedProfiles::all(),
            default_profile: "default".to_string(),
        }
    }

    pub fn select_for_task(&self, task_type: &str) -> Option<&AgentProfile> {
        self.profiles
            .iter()
            .filter(|p| {
                p.task_types
                    .iter()
                    .any(|t| t == "*" || t == task_type || task_type.contains(t.as_str()))
            })
            .max_by_key(|p| p.priority)
    }

    pub fn select_by_name(&self, name: &str) -> Option<&AgentProfile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn default_profile(&self) -> Option<&AgentProfile> {
        self.select_by_name(&self.default_profile)
    }
}

impl Default for AgentRoleSelector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_env_ref_parsing() {
        // Test with env:// prefix
        let result = parse_env_ref("env://NON_EXISTENT_VAR_12345");
        assert!(result.is_none()); // Should be None since var doesn't exist

        // Test without env:// prefix
        assert_eq!(parse_env_ref("plain_text"), Some("plain_text".to_string()));
    }

    #[test]
    fn test_config_layer_paths() {
        assert_eq!(ConfigLayer::Global.path(), PathBuf::from("/etc/ndc"));
        assert!(
            ConfigLayer::User
                .path()
                .to_string_lossy()
                .contains(".config/ndc")
        );
        assert_eq!(ConfigLayer::Project.path(), PathBuf::from(".ndc"));
    }

    #[test]
    fn test_yaml_agent_profile_defaults() {
        let profile = YamlAgentProfile::default();
        assert_eq!(profile.name, "default");
        assert!(profile.enable_streaming);
    }
}
