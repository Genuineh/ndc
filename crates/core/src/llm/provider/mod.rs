//! LLM Provider Abstraction
//!
//! Responsibilities:
//! - Unified Provider trait for all LLM backends
//! - Common types and structures
//! - Request/Response handling
//! - Token counting
//! - Model registry

pub mod anthropic;
pub mod minimax;
pub mod openai;
pub mod openrouter;
pub mod token_counter;

pub use anthropic::{AnthropicProvider, create_anthropic_config};
pub use minimax::{MiniMaxProvider, create_minimax_config};
pub use openai::{OpenAiProvider, create_azure_config, create_openai_config};
pub use openrouter::{OpenRouterProvider, create_openrouter_config};
pub use token_counter::{SimpleTokenCounter, TokenCountError};

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Provider-specific errors
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("API error: {message}")]
    Api {
        message: String,
        status_code: Option<u16>,
    },

    #[error("Rate limited, retry after {retry_after}s")]
    RateLimited { retry_after: u64 },

    #[error("Authentication failed: {message}")]
    Auth { message: String },

    #[error("Model not found: {model}")]
    ModelNotFound { model: String },

    #[error("Context length exceeded: {length} > {max_length}")]
    ContextLengthExceeded { length: usize, max_length: usize },

    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },

    #[error("Network error: {source}")]
    Network { source: reqwest::Error },

    #[error("Unknown provider: {name}")]
    UnknownProvider { name: String },

    #[error("Invalid config: {message}")]
    InvalidConfig { message: String },
}

/// Message role
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "tool")]
    Tool,
}

/// A single message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    #[serde(default, deserialize_with = "deserialize_message_content")]
    pub content: String,
    pub name: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

fn deserialize_message_content<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let content = Option::<String>::deserialize(deserializer)?;
    Ok(content.unwrap_or_default())
}

/// Tool call request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: ToolCallFunction,
}

/// Tool call function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// Tool call result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,
}

/// Complete request to LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub stop: Option<Vec<String>>,
    pub stream: bool,
    pub tools: Option<Vec<serde_json::Value>>,
}

/// Response from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

/// Choice in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: usize,
    pub message: Message,
    pub finish_reason: Option<String>,
    pub logprobs: Option<HashMap<String, serde_json::Value>>,
}

/// Token usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Stream chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<StreamChoice>,
}

/// Stream choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChoice {
    pub index: usize,
    pub delta: Option<Message>,
    pub finish_reason: Option<String>,
}

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
    pub permission: Vec<ModelPermission>,
}

/// Model permission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPermission {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub allow_create_engine: bool,
    pub allow_sampling: bool,
    pub allow_logprobs: bool,
    pub allow_search_indices: bool,
    pub allow_view: bool,
    pub allow_fine_tuning: bool,
    pub organization: String,
    pub group: Option<String>,
    pub is_blocking: bool,
}

/// Provider configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub provider_type: ProviderType,
    pub api_key: String,
    pub base_url: Option<String>,
    pub organization: Option<String>,
    pub default_model: String,
    pub models: Vec<String>,
    pub timeout_ms: u64,
    pub max_retries: u32,
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let masked_key = if self.api_key.len() > 8 {
            format!("{}***", &self.api_key[..4])
        } else {
            "***".to_string()
        };
        f.debug_struct("ProviderConfig")
            .field("name", &self.name)
            .field("provider_type", &self.provider_type)
            .field("api_key", &masked_key)
            .field("base_url", &self.base_url)
            .field("organization", &self.organization)
            .field("default_model", &self.default_model)
            .field("models", &self.models)
            .field("timeout_ms", &self.timeout_ms)
            .field("max_retries", &self.max_retries)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "azure")]
    Azure,
    #[serde(rename = "ollama")]
    Ollama,
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "minimax")]
    MiniMax,
    #[serde(rename = "openrouter")]
    OpenRouter,
}

impl From<String> for ProviderType {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "openai" => ProviderType::OpenAi,
            "anthropic" => ProviderType::Anthropic,
            "azure" => ProviderType::Azure,
            "ollama" => ProviderType::Ollama,
            "local" => ProviderType::Local,
            "minimax" => ProviderType::MiniMax,
            "openrouter" => ProviderType::OpenRouter,
            _ => ProviderType::OpenAi, // 默认使用 OpenAi
        }
    }
}

impl From<&str> for ProviderType {
    fn from(s: &str) -> Self {
        s.to_string().into()
    }
}

/// Streaming callback
#[async_trait::async_trait]
pub trait StreamHandler: Send + Sync {
    async fn on_chunk(&self, chunk: &StreamChunk) -> Result<(), ProviderError>;
    async fn on_complete(&self, response: &CompletionResponse) -> Result<(), ProviderError>;
    async fn on_error(&self, error: &ProviderError);
}

/// Token counter trait
#[async_trait::async_trait]
pub trait TokenCounter: Send + Sync {
    fn count_messages(&self, messages: &[Message], model: &str) -> usize;
    fn count_text(&self, text: &str, model: &str) -> usize;
    fn get_max_tokens(&self, model: &str) -> usize;
}

/// LLM Provider trait
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Get provider type
    fn provider_type(&self) -> ProviderType;

    /// Get provider name
    fn name(&self) -> &str;

    /// List available models
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError>;

    /// Complete a request (non-streaming)
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError>;

    /// Complete a request with streaming
    async fn complete_streaming(
        &self,
        request: &CompletionRequest,
        handler: &Arc<dyn StreamHandler>,
    ) -> Result<(), ProviderError>;

    /// Get token usage for a request
    fn estimate_tokens(&self, request: &CompletionRequest) -> Usage;

    /// Check if model is available
    async fn is_model_available(&self, model: &str) -> bool;

    /// Get configuration
    fn config(&self) -> &ProviderConfig;
}

/// No-op stream handler
#[derive(Debug, Clone, Default)]
pub struct NoOpStreamHandler;

#[async_trait::async_trait]
impl StreamHandler for NoOpStreamHandler {
    async fn on_chunk(&self, _chunk: &StreamChunk) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn on_complete(&self, _response: &CompletionResponse) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn on_error(&self, _error: &ProviderError) {}
}

/// Convert provider error from external error
pub fn map_provider_error(error: reqwest::Error, _provider: &str) -> ProviderError {
    if let Some(status) = error.status() {
        match status.as_u16() {
            401 => ProviderError::Auth {
                message: "Invalid API key".to_string(),
            },
            403 => ProviderError::Auth {
                message: "Access denied".to_string(),
            },
            404 => ProviderError::Api {
                message: "Resource not found".to_string(),
                status_code: Some(401),
            },
            429 => ProviderError::RateLimited { retry_after: 60 },
            500 | 502 | 503 | 504 => ProviderError::Api {
                message: "Server error".to_string(),
                status_code: Some(status.as_u16()),
            },
            _ => ProviderError::Api {
                message: error.to_string(),
                status_code: Some(status.as_u16()),
            },
        }
    } else {
        ProviderError::Network { source: error }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_config_serde() {
        let config = ProviderConfig {
            name: "test-openai".to_string(),
            provider_type: ProviderType::OpenAi,
            api_key: "sk-test".to_string(),
            base_url: None,
            organization: None,
            default_model: "gpt-4".to_string(),
            models: vec!["gpt-4".to_string()],
            timeout_ms: 60000,
            max_retries: 3,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: ProviderConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "test-openai");
        assert_eq!(parsed.provider_type, ProviderType::OpenAi);
    }

    #[test]
    fn test_message_serde() {
        let message = Message {
            role: MessageRole::User,
            content: "Hello!".to_string(),
            name: None,
            tool_calls: None,
        };

        let json = serde_json::to_string(&message).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.role, MessageRole::User);
        assert_eq!(parsed.content, "Hello!");
    }

    #[test]
    fn test_completion_request_serde() {
        let request = CompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                Message {
                    role: MessageRole::System,
                    content: "You are a helpful assistant.".to_string(),
                    name: None,
                    tool_calls: None,
                },
                Message {
                    role: MessageRole::User,
                    content: "Hello!".to_string(),
                    name: None,
                    tool_calls: None,
                },
            ],
            temperature: Some(0.7),
            max_tokens: Some(100),
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            stream: false,
            tools: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: CompletionRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.model, "gpt-4");
        assert_eq!(parsed.messages.len(), 2);
        assert_eq!(parsed.temperature, Some(0.7));
        assert!(parsed.tools.is_none());
    }

    #[test]
    fn test_message_null_content_deserialization() {
        let json = r#"{"role":"assistant","content":null}"#;
        let parsed: Message = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.role, MessageRole::Assistant);
        assert_eq!(parsed.content, "");
    }

    #[test]
    fn test_usage_serde() {
        let usage = Usage {
            prompt_tokens: 10,
            completion_tokens: 50,
            total_tokens: 60,
        };

        let json = serde_json::to_string(&usage).unwrap();
        let parsed: Usage = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.prompt_tokens, 10);
        assert_eq!(parsed.completion_tokens, 50);
        assert_eq!(parsed.total_tokens, 60);
    }

    #[test]
    fn test_tool_call_serde() {
        let tool_call = ToolCall {
            id: "call-1".to_string(),
            function: ToolCallFunction {
                name: "search".to_string(),
                arguments: r#"{"query": "test"}"#.to_string(),
            },
        };

        let json = serde_json::to_string(&tool_call).unwrap();
        let parsed: ToolCall = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "call-1");
        assert_eq!(parsed.function.name, "search");
    }

    #[test]
    fn test_provider_type_variants() {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct Test {
            ptype: ProviderType,
        }

        let json = r#"{"ptype": "openai"}"#;
        let parsed: Test = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.ptype, ProviderType::OpenAi);
    }

    #[test]
    fn test_message_role_variants() {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct Test {
            role: MessageRole,
        }

        let json = r#"{"role": "user"}"#;
        let parsed: Test = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.role, MessageRole::User);
    }
}
