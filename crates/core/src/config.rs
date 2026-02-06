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

/// Provider 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    Openai,
    Anthropic,
    Minimax,
    Ollama,
    Custom,
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
