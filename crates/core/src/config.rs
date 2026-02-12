//! NDC 配置系统
//!
//! 支持 YAML 配置文件和环境变量

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;

/// NDC 主配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NdcConfig {
    /// LLM 配置
    pub llm: Option<LlmConfig>,

    /// REPL 配置
    pub repl: Option<ReplConfig>,

    /// Runtime 配置
    pub runtime: Option<RuntimeConfig>,

    /// 存储配置
    pub storage: Option<StorageConfig>,
}

impl Default for NdcConfig {
    fn default() -> Self {
        Self {
            llm: None,
            repl: Some(ReplConfig::default()),
            runtime: Some(RuntimeConfig::default()),
            storage: Some(StorageConfig::default()),
        }
    }
}

/// LLM 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// 是否启用 LLM
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// 默认 Provider
    #[serde(default = "default_provider")]
    pub provider: String,

    /// 默认模型
    #[serde(default = "default_model")]
    pub model: String,

    /// API 基础 URL
    pub base_url: Option<String>,

    /// API Key (支持环境变量引用)
    pub api_key: Option<String>,

    /// 组织 ID (可选)
    pub organization: Option<String>,

    /// 温度参数
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// 最大 token 数
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// 请求超时 (秒)
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Provider 列表
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
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

impl Default for LlmConfig {
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

/// Provider 配置 (YAML 反序列化用)
/// 注意：与 llm/provider/mod.rs 中的 ProviderConfig 不同
/// 此版本用于 YAML 配置文件，所有字段为 Option
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "YamlProviderConfigHelper", into = "YamlProviderConfigHelper")]
pub struct ProviderConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub organization: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub timeout: Option<u64>,
    pub capabilities: Option<Vec<String>>,
}

/// Helper for YAML serialization/deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
struct YamlProviderConfigHelper {
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

impl From<YamlProviderConfigHelper> for ProviderConfig {
    fn from(helper: YamlProviderConfigHelper) -> Self {
        Self {
            name: helper.name,
            provider_type: match helper.provider_type.as_str() {
                "openai" | "OpenAi" => ProviderType::OpenAi,
                "anthropic" | "Anthropic" => ProviderType::Anthropic,
                "azure" | "Azure" => ProviderType::Azure,
                "ollama" | "Ollama" => ProviderType::Ollama,
                "minimax" | "MiniMax" => ProviderType::MiniMax,
                "openrouter" | "OpenRouter" => ProviderType::OpenRouter,
                "local" | "Local" => ProviderType::Local,
                _ => ProviderType::Custom(helper.provider_type),
            },
            model: helper.model,
            base_url: helper.base_url,
            api_key: helper.api_key,
            organization: helper.organization,
            temperature: helper.temperature,
            max_tokens: helper.max_tokens,
            timeout: helper.timeout,
            capabilities: helper.capabilities,
        }
    }
}

impl From<ProviderConfig> for YamlProviderConfigHelper {
    fn from(config: ProviderConfig) -> Self {
        Self {
            name: config.name,
            provider_type: match &config.provider_type {
                ProviderType::OpenAi => "openai".to_string(),
                ProviderType::Anthropic => "anthropic".to_string(),
                ProviderType::Azure => "azure".to_string(),
                ProviderType::Ollama => "ollama".to_string(),
                ProviderType::MiniMax => "minimax".to_string(),
                ProviderType::OpenRouter => "openrouter".to_string(),
                ProviderType::Local => "local".to_string(),
                ProviderType::Custom(s) => s.clone(),
            },
            model: config.model,
            base_url: config.base_url,
            api_key: config.api_key,
            organization: config.organization,
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            timeout: config.timeout,
            capabilities: config.capabilities,
        }
    }
}

/// Provider 类型枚举 (YAML 配置用)
/// 注意：与 llm/provider/mod.rs 中的 ProviderType 保持一致
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ProviderType {
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "azure")]
    Azure,
    #[serde(rename = "ollama")]
    Ollama,
    #[serde(rename = "minimax")]
    MiniMax,
    #[serde(rename = "openrouter")]
    OpenRouter,
    #[serde(rename = "local")]
    Local,
    Custom(String),
}

impl From<String> for ProviderType {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "openai" => ProviderType::OpenAi,
            "anthropic" => ProviderType::Anthropic,
            "azure" => ProviderType::Azure,
            "ollama" => ProviderType::Ollama,
            "minimax" => ProviderType::MiniMax,
            "openrouter" => ProviderType::OpenRouter,
            "local" => ProviderType::Local,
            _ => ProviderType::Custom(s),
        }
    }
}

impl From<ProviderType> for String {
    fn from(pt: ProviderType) -> Self {
        match pt {
            ProviderType::OpenAi => "openai".to_string(),
            ProviderType::Anthropic => "anthropic".to_string(),
            ProviderType::Azure => "azure".to_string(),
            ProviderType::Ollama => "ollama".to_string(),
            ProviderType::MiniMax => "minimax".to_string(),
            ProviderType::OpenRouter => "openrouter".to_string(),
            ProviderType::Local => "local".to_string(),
            ProviderType::Custom(s) => s,
        }
    }
}

/// OpenAI 专用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    pub api_key: Option<String>,
    pub organization: Option<String>,
    pub project: Option<String>,
    pub base_url: Option<String>,
}

/// Anthropic 专用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub version: Option<String>,
}

/// MiniMax 专用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiniMaxConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

/// Ollama 专用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub base_url: Option<String>,
    pub timeout: Option<u64>,
}

/// REPL 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplConfig {
    /// 提示符
    #[serde(default = "default_prompt")]
    pub prompt: String,

    /// 历史文件
    pub history_file: Option<PathBuf>,

    /// 最大历史行数
    #[serde(default = "default_max_history")]
    pub max_history: usize,

    /// 是否显示思考过程
    #[serde(default = "default_true")]
    pub show_thought: bool,

    /// 自动创建任务
    #[serde(default = "default_true")]
    pub auto_create_task: bool,

    /// 会话超时 (秒)
    #[serde(default = "default_session_timeout")]
    pub session_timeout: u64,

    /// LLM 回退到正则
    #[serde(default = "default_true")]
    pub fallback_to_regex: bool,

    /// 确认模式 (危险操作需要确认)
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

impl Default for ReplConfig {
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
pub struct RuntimeConfig {
    /// 并发任务数
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_tasks: usize,

    /// 执行超时 (秒)
    #[serde(default = "default_execution_timeout")]
    pub execution_timeout: u64,

    /// 工作目录
    pub working_dir: Option<PathBuf>,

    /// 质量门禁
    pub quality_gates: Option<Vec<String>>,
}

fn default_max_concurrent() -> usize {
    4
}

fn default_execution_timeout() -> u64 {
    300
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: default_max_concurrent(),
            execution_timeout: default_execution_timeout(),
            working_dir: None,
            quality_gates: None,
        }
    }
}

/// 存储配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// 存储类型
    #[serde(default = "default_storage_type")]
    pub storage_type: String,

    /// 数据库路径
    pub db_path: Option<PathBuf>,

    /// 内存存储
    #[serde(default)]
    pub in_memory: bool,
}

fn default_storage_type() -> String {
    "memory".to_string()
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            storage_type: default_storage_type(),
            db_path: None,
            in_memory: true,
        }
    }
}

// ============================================================================
// Agent Configuration System
// ============================================================================

/// Agent profile configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Profile name (e.g., "default", "implementer", "verifier")
    pub name: String,

    /// Display name
    pub display_name: String,

    /// Description
    pub description: String,

    /// LLM provider to use
    pub provider: String,

    /// Model name
    pub model: String,

    /// Temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Maximum tokens
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Maximum tool calls per session
    #[serde(default = "default_max_tool_calls")]
    pub max_tool_calls: usize,

    /// Enable streaming
    #[serde(default = "default_true")]
    pub enable_streaming: bool,

    /// Auto-verify results
    #[serde(default = "default_true")]
    pub auto_verify: bool,

    /// Tool permissions (allow/ask/deny per tool)
    #[serde(default)]
    pub tool_permissions: ToolPermissions,

    /// Task types this agent handles
    #[serde(default)]
    pub task_types: Vec<String>,

    /// Priority (higher = preferred)
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
            description: "General purpose agent with balanced settings".to_string(),
            provider: default_provider(),
            model: default_model(),
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

/// Tool permission rules
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolPermissions {
    /// Default rule for unmapped tools
    #[serde(default)]
    pub default: PermissionRule,

    /// Specific tool rules
    #[serde(default)]
    pub tools: HashMap<String, PermissionRule>,
}

/// Permission rule for tools
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionRule {
    /// Tool is always allowed
    Allow,
    /// Ask user for confirmation
    Ask,
    /// Tool is denied
    Deny,
}

impl Default for PermissionRule {
    fn default() -> Self {
        Self::Ask
    }
}

/// Predefined agent profiles
pub struct PredefinedProfiles;

impl PredefinedProfiles {
    pub fn default() -> AgentProfile {
        AgentProfile {
            name: "default".to_string(),
            display_name: "Default Agent".to_string(),
            description: "General purpose agent with balanced settings".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            temperature: 0.1,
            max_tokens: 4096,
            max_tool_calls: 50,
            enable_streaming: true,
            auto_verify: true,
            tool_permissions: ToolPermissions {
                default: PermissionRule::Allow,
                tools: {
                    let mut map = HashMap::new();
                    map.insert("file_write".to_string(), PermissionRule::Ask);
                    map.insert("file_delete".to_string(), PermissionRule::Ask);
                    map.insert("git_commit".to_string(), PermissionRule::Ask);
                    map.insert("git_push".to_string(), PermissionRule::Ask);
                    map
                },
            },
            task_types: vec!["*".to_string()],
            priority: 0,
        }
    }

    pub fn implementer() -> AgentProfile {
        AgentProfile {
            name: "implementer".to_string(),
            display_name: "Code Implementer".to_string(),
            description: "Specialized for implementing features and bug fixes".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            temperature: 0.1,
            max_tokens: 8192,
            max_tool_calls: 100,
            enable_streaming: true,
            auto_verify: true,
            tool_permissions: ToolPermissions {
                default: PermissionRule::Allow,
                tools: {
                    let mut map = HashMap::new();
                    map.insert("file_delete".to_string(), PermissionRule::Ask);
                    map.insert("git_push".to_string(), PermissionRule::Ask);
                    map
                },
            },
            task_types: vec![
                "implementation".to_string(),
                "bugfix".to_string(),
                "refactor".to_string(),
            ],
            priority: 10,
        }
    }

    pub fn verifier() -> AgentProfile {
        AgentProfile {
            name: "verifier".to_string(),
            display_name: "Code Verifier".to_string(),
            description: "Specialized for verifying and reviewing code".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            temperature: 0.0,
            max_tokens: 4096,
            max_tool_calls: 30,
            enable_streaming: false,
            auto_verify: false,
            tool_permissions: ToolPermissions {
                default: PermissionRule::Allow,
                tools: HashMap::new(),
            },
            task_types: vec![
                "verification".to_string(),
                "review".to_string(),
                "testing".to_string(),
            ],
            priority: 5,
        }
    }

    pub fn planner() -> AgentProfile {
        AgentProfile {
            name: "planner".to_string(),
            display_name: "Task Planner".to_string(),
            description: "Specialized for breaking down complex tasks".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-opus-4-6".to_string(),
            temperature: 0.2,
            max_tokens: 16384,
            max_tool_calls: 50,
            enable_streaming: true,
            auto_verify: false,
            tool_permissions: ToolPermissions {
                default: PermissionRule::Allow,
                tools: {
                    let mut map = HashMap::new();
                    map.insert("file_write".to_string(), PermissionRule::Deny);
                    map.insert("file_delete".to_string(), PermissionRule::Deny);
                    map.insert("git_commit".to_string(), PermissionRule::Deny);
                    map
                },
            },
            task_types: vec![
                "planning".to_string(),
                "design".to_string(),
                "architecture".to_string(),
            ],
            priority: 15,
        }
    }

    /// Get all predefined profiles
    pub fn all() -> Vec<AgentProfile> {
        vec![
            Self::default(),
            Self::implementer(),
            Self::verifier(),
            Self::planner(),
        ]
    }
}

/// Agent role selector - selects appropriate agent based on task type
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

    /// Add a custom profile
    pub fn add_profile(&mut self, profile: AgentProfile) {
        self.profiles.push(profile);
    }

    /// Select agent for task type
    pub fn select_for_task(&self, task_type: &str) -> Option<&AgentProfile> {
        self.profiles
            .iter()
            .filter(|p| {
                p.task_types.iter().any(|t| {
                    t == "*" || t == task_type || task_type.contains(t.as_str())
                })
            })
            .max_by_key(|p| p.priority)
    }

    /// Select agent by name
    pub fn select_by_name(&self, name: &str) -> Option<&AgentProfile> {
        self.profiles.iter().find(|p| p.name == name)
            .or_else(|| self.select_by_name(&self.default_profile))
    }

    /// Set default profile
    pub fn set_default(&mut self, name: String) {
        self.default_profile = name;
    }

    /// List all profiles
    pub fn list_profiles(&self) -> &[AgentProfile] {
        &self.profiles
    }
}

impl Default for AgentRoleSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// Agent configuration directory
#[derive(Debug, Clone)]
pub struct AgentConfigDir {
    /// Config directory path
    path: PathBuf,
}

impl AgentConfigDir {
    /// Create config directory at default location
    pub fn default() -> Result<Self, std::io::Error> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());

        let path = PathBuf::from(home).join(".ndc").join("config").join("agents");

        // Create directory if not exists
        std::fs::create_dir_all(&path)?;

        Ok(Self { path })
    }

    /// Create config directory at custom path
    pub fn with_path(path: PathBuf) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// Save profile to file
    pub fn save_profile(&self, profile: &AgentProfile) -> Result<(), std::io::Error> {
        let file_path = self.path.join(format!("{}.yaml", profile.name));
        let yaml = serde_yaml::to_string(profile)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(file_path, yaml)
    }

    /// Load profile from file
    pub fn load_profile(&self, name: &str) -> Result<Option<AgentProfile>, std::io::Error> {
        let file_path = self.path.join(format!("{}.yaml", name));

        if !file_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&file_path)?;
        let profile: AgentProfile = serde_yaml::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        Ok(Some(profile))
    }

    /// List all profiles
    pub fn list_profiles(&self) -> Result<Vec<String>, std::io::Error> {
        let mut profiles = Vec::new();

        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    profiles.push(name.to_string());
                }
            }
        }

        Ok(profiles)
    }

    /// Initialize default profiles
    pub fn initialize_defaults(&self) -> Result<(), std::io::Error> {
        for profile in PredefinedProfiles::all() {
            // Only save if doesn't exist
            if self.load_profile(&profile.name)?.is_none() {
                self.save_profile(&profile)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_agent_profile_default() {
        let profile = AgentProfile::default();
        assert_eq!(profile.name, "default");
        assert_eq!(profile.provider, "openai");
        assert_eq!(profile.model, "gpt-4o");
    }

    #[test]
    fn test_permission_rule_default() {
        let rule = PermissionRule::default();
        assert_eq!(rule, PermissionRule::Ask);
    }

    #[test]
    fn test_predefined_profiles() {
        let profiles = PredefinedProfiles::all();
        assert_eq!(profiles.len(), 4);

        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"implementer"));
        assert!(names.contains(&"verifier"));
        assert!(names.contains(&"planner"));
    }

    #[test]
    fn test_agent_role_selector() {
        let selector = AgentRoleSelector::new();

        // Select by task type
        let implementer = selector.select_for_task("implementation");
        assert!(implementer.is_some());
        assert_eq!(implementer.unwrap().name, "implementer");

        let verifier = selector.select_for_task("verification");
        assert!(verifier.is_some());
        assert_eq!(verifier.unwrap().name, "verifier");
    }

    #[test]
    fn test_tool_permissions() {
        let permissions = ToolPermissions {
            default: PermissionRule::Allow,
            tools: {
                let mut map = HashMap::new();
                map.insert("file_delete".to_string(), PermissionRule::Ask);
                map
            },
        };

        assert_eq!(permissions.default, PermissionRule::Allow);
        assert_eq!(permissions.tools.get("file_delete"), Some(&PermissionRule::Ask));
    }
}
