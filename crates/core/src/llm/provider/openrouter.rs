//! OpenRouter Provider Implementation
//!
//! Supports:
//! - OpenRouter API (Unified access to multiple LLM models)
//! - Streaming responses
//! - Model list API to fetch available models
//! - Multiple providers through single API
//!
//! API Documentation: https://openrouter.ai/docs

use super::*;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use std::sync::Arc;

/// OpenRouter API base URL
const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// OpenRouter Provider
#[derive(Clone)]
pub struct OpenRouterProvider {
    config: ProviderConfig,
    client: Client,
    token_counter: Arc<dyn TokenCounter>,
    site_url: Option<String>,
    app_name: Option<String>,
}

impl std::fmt::Debug for OpenRouterProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenRouterProvider")
            .field("name", &self.config.name)
            .field("default_model", &self.config.default_model)
            .field("site_url", &self.site_url)
            .field("app_name", &self.app_name)
            .finish_non_exhaustive()
    }
}

impl OpenRouterProvider {
    /// Create a new OpenRouter provider
    pub fn new(config: ProviderConfig, token_counter: Arc<dyn TokenCounter>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            client,
            token_counter,
            site_url: None,
            app_name: None,
        }
    }

    /// Create OpenRouter provider with site info
    pub fn with_site_info(
        config: ProviderConfig,
        token_counter: Arc<dyn TokenCounter>,
        site_url: Option<String>,
        app_name: Option<String>,
    ) -> Self {
        let mut provider = Self::new(config, token_counter);
        provider.site_url = site_url;
        provider.app_name = app_name;
        provider
    }

    /// Get base URL for API calls
    fn get_base_url(&self) -> String {
        if let Some(url) = &self.config.base_url {
            url.clone()
        } else {
            OPENROUTER_BASE_URL.to_string()
        }
    }

    /// Build authorization header
    fn get_auth_header(&self) -> String {
        format!("Bearer {}", self.config.api_key)
    }

    /// Build request headers for OpenRouter
    fn build_request_headers(&self) -> Vec<(&'static str, String)> {
        let mut headers = vec![
            (
                "HTTP-Referer",
                self.site_url
                    .clone()
                    .unwrap_or_else(|| "https://github.com/Genuineh/ndc".to_string()),
            ),
            (
                "X-Title",
                self.app_name.clone().unwrap_or_else(|| "NDC".to_string()),
            ),
        ];

        // Add custom headers from organization if provided
        if let Some(ref org) = self.config.organization {
            headers.push(("X-Organization", org.clone()));
        }

        headers
    }

    fn serialize_messages(&self, request: &CompletionRequest) -> Vec<serde_json::Value> {
        request
            .messages
            .iter()
            .map(|m| {
                let mut msg = serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                });

                if let Some(name) = &m.name {
                    if m.role == MessageRole::Tool {
                        msg["tool_call_id"] = serde_json::json!(name);
                    } else {
                        msg["name"] = serde_json::json!(name);
                    }
                }

                if let Some(calls) = &m.tool_calls
                    && !calls.is_empty() {
                        msg["tool_calls"] = serde_json::json!(calls);
                    }

                msg
            })
            .collect()
    }

    fn apply_tools(&self, body: &mut serde_json::Value, request: &CompletionRequest) {
        if let Some(tools) = request.tools.as_ref().filter(|t| !t.is_empty()) {
            body["tools"] = serde_json::json!(tools);
            body["tool_choice"] = serde_json::json!("auto");
        }
    }

    /// Parse OpenRouter response to CompletionResponse
    fn parse_response(
        &self,
        response_value: serde_json::Value,
    ) -> Result<CompletionResponse, ProviderError> {
        let choices = response_value["choices"]
            .as_array()
            .ok_or_else(|| ProviderError::Api {
                message: "Missing choices in response".to_string(),
                status_code: None,
            })?;

        let first_choice = choices.first().ok_or_else(|| ProviderError::Api {
            message: "Empty choices array".to_string(),
            status_code: None,
        })?;

        let message = first_choice["message"]
            .as_object()
            .ok_or_else(|| ProviderError::Api {
                message: "Invalid message format".to_string(),
                status_code: None,
            })?;

        let content = message
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let role = message
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("assistant");
        let tool_calls = message
            .get("tool_calls")
            .and_then(|v| serde_json::from_value::<Vec<ToolCall>>(v.clone()).ok());

        // Parse usage if available
        let usage = response_value.get("usage").map(|u| Usage {
                prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
                total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
            });

        let finish_reason = first_choice
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("stop")
            .to_string();

        // Parse message role
        let message_role = match role {
            "system" => MessageRole::System,
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "tool" => MessageRole::Tool,
            _ => MessageRole::Assistant,
        };

        Ok(CompletionResponse {
            id: response_value["id"]
                .as_str()
                .unwrap_or("openrouter-unknown")
                .to_string(),
            object: response_value["object"]
                .as_str()
                .unwrap_or("chat.completion")
                .to_string(),
            created: response_value["created"].as_u64().unwrap_or(0),
            model: response_value["model"]
                .as_str()
                .unwrap_or(&self.config.default_model)
                .to_string(),
            choices: vec![Choice {
                index: first_choice["index"].as_u64().unwrap_or(0) as usize,
                message: Message {
                    role: message_role,
                    content: content.to_string(),
                    name: None,
                    tool_calls,
                },
                finish_reason: Some(finish_reason),
                logprobs: None,
            }],
            usage,
        })
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenRouterProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::OpenRouter
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let url = format!("{}/models", self.get_base_url());

        let mut req_builder = self
            .client
            .get(&url)
            .header("Authorization", self.get_auth_header());

        // Add OpenRouter specific headers
        for (key, value) in self.build_request_headers() {
            req_builder = req_builder.header(key, value);
        }

        let response = req_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network { source: e })?;

        if !response.status().is_success() {
            return Err(ProviderError::Api {
                message: format!("Failed to fetch models: {}", response.status()),
                status_code: Some(response.status().as_u16()),
            });
        }

        let data: serde_json::Value = response.json().await.map_err(|e| ProviderError::Api {
            message: format!("Failed to parse models response: {}", e),
            status_code: None,
        })?;

        let models_array =
            data.get("data")
                .and_then(|v| v.as_array())
                .ok_or_else(|| ProviderError::Api {
                    message: "Missing data array in models response".to_string(),
                    status_code: None,
                })?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let models: Vec<ModelInfo> = models_array
            .iter()
            .filter_map(|m| {
                Some(ModelInfo {
                    id: m.get("id")?.as_str()?.to_string(),
                    object: m
                        .get("object")
                        .and_then(|v| v.as_str())
                        .unwrap_or("model")
                        .to_string(),
                    created: m.get("created").and_then(|v| v.as_u64()).unwrap_or(now),
                    owned_by: m
                        .get("owned_by")
                        .and_then(|v| v.as_str())
                        .unwrap_or("openrouter")
                        .to_string(),
                    permission: vec![],
                })
            })
            .collect();

        Ok(models)
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.get_base_url());

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": self.serialize_messages(request),
            "temperature": request.temperature.unwrap_or(0.7),
            "max_tokens": request.max_tokens,
            "top_p": request.top_p,
            "frequency_penalty": request.frequency_penalty,
            "presence_penalty": request.presence_penalty,
            "stop": request.stop,
            "stream": false,
        });
        self.apply_tools(&mut body, request);

        let mut req_builder = self
            .client
            .post(&url)
            .header("Authorization", self.get_auth_header())
            .header("Content-Type", "application/json");

        // Add OpenRouter specific headers
        for (key, value) in self.build_request_headers() {
            req_builder = req_builder.header(key, value);
        }

        let response = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network { source: e })?;

        let status = response.status();

        if status == StatusCode::UNAUTHORIZED {
            return Err(ProviderError::Auth {
                message: "Invalid OpenRouter API key".to_string(),
            });
        }

        if status.as_u16() == 429 {
            return Err(ProviderError::RateLimited { retry_after: 60 });
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            return Err(ProviderError::Api {
                message: format!("OpenRouter API error: {}", error_text),
                status_code: Some(status.as_u16()),
            });
        }

        let response_value: serde_json::Value =
            response.json().await.map_err(|e| ProviderError::Api {
                message: format!("Failed to parse response: {}", e),
                status_code: None,
            })?;

        self.parse_response(response_value)
    }

    async fn complete_streaming(
        &self,
        request: &CompletionRequest,
        handler: &Arc<dyn StreamHandler>,
    ) -> Result<(), ProviderError> {
        let url = format!("{}/chat/completions", self.get_base_url());

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": self.serialize_messages(request),
            "temperature": request.temperature.unwrap_or(0.7),
            "max_tokens": request.max_tokens,
            "top_p": request.top_p,
            "frequency_penalty": request.frequency_penalty,
            "presence_penalty": request.presence_penalty,
            "stop": request.stop,
            "stream": true,
        });
        self.apply_tools(&mut body, request);

        let mut req_builder = self
            .client
            .post(&url)
            .header("Authorization", self.get_auth_header())
            .header("Content-Type", "application/json");

        // Add OpenRouter specific headers
        for (key, value) in self.build_request_headers() {
            req_builder = req_builder.header(key, value);
        }

        let response = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network { source: e })?;

        let status = response.status();

        if status == StatusCode::UNAUTHORIZED {
            return Err(ProviderError::Auth {
                message: "Invalid OpenRouter API key".to_string(),
            });
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ProviderError::Api {
                message: format!("OpenRouter API error: {}", error_text),
                status_code: Some(status.as_u16()),
            });
        }

        let mut stream = response.bytes_stream();

        let mut full_response: Option<CompletionResponse> = None;
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| ProviderError::Network { source: e })?;
            let text = std::str::from_utf8(&chunk).map_err(|e| ProviderError::Api {
                message: e.to_string(),
                status_code: None,
            })?;

            buffer.push_str(text);

            // OpenRouter SSE format: data: {...}
            for line in buffer.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        if let Some(ref response) = full_response {
                            handler.on_complete(response).await?;
                        }
                        return Ok(());
                    }

                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
                        // OpenRouter streaming format follows OpenAI format
                        let delta = value["choices"].get(0).and_then(|c| c.get("delta"));

                        if let Some(delta_obj) = delta {
                            let chunk_content = delta_obj
                                .get("content")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");

                            if !chunk_content.is_empty() {
                                let chunk_role = delta_obj
                                    .get("role")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("assistant");

                                let stream_chunk = StreamChunk {
                                    id: value["id"].as_str().unwrap_or("").to_string(),
                                    object: value["object"]
                                        .as_str()
                                        .unwrap_or("chat.completion.chunk")
                                        .to_string(),
                                    created: value["created"].as_u64().unwrap_or(0),
                                    model: value["model"]
                                        .as_str()
                                        .unwrap_or(&self.config.default_model)
                                        .to_string(),
                                    choices: vec![StreamChoice {
                                        index: value["choices"]
                                            .get(0)
                                            .and_then(|c| c.get("index"))
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0)
                                            as usize,
                                        delta: Some(Message {
                                            role: match chunk_role {
                                                "system" => MessageRole::System,
                                                "user" => MessageRole::User,
                                                "assistant" => MessageRole::Assistant,
                                                "tool" => MessageRole::Tool,
                                                _ => MessageRole::Assistant,
                                            },
                                            content: chunk_content.to_string(),
                                            name: None,
                                            tool_calls: None,
                                        }),
                                        finish_reason: None,
                                    }],
                                };

                                handler.on_chunk(&stream_chunk).await?;

                                // Build full response progressively
                                if full_response.is_none() {
                                    full_response = Some(CompletionResponse {
                                        id: value["id"].as_str().unwrap_or("").to_string(),
                                        object: value["object"]
                                            .as_str()
                                            .unwrap_or("chat.completion")
                                            .to_string(),
                                        created: value["created"].as_u64().unwrap_or(0),
                                        model: value["model"]
                                            .as_str()
                                            .unwrap_or(&self.config.default_model)
                                            .to_string(),
                                        choices: vec![Choice {
                                            index: 0,
                                            message: Message {
                                                role: match chunk_role {
                                                    "system" => MessageRole::System,
                                                    "user" => MessageRole::User,
                                                    "assistant" => MessageRole::Assistant,
                                                    "tool" => MessageRole::Tool,
                                                    _ => MessageRole::Assistant,
                                                },
                                                content: chunk_content.to_string(),
                                                name: None,
                                                tool_calls: None,
                                            },
                                            finish_reason: None,
                                            logprobs: None,
                                        }],
                                        usage: None,
                                    });
                                } else if let Some(ref mut resp) = full_response {
                                    // Append content
                                    if let Some(choice) = resp.choices.first_mut() {
                                        choice.message.content.push_str(chunk_content);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Keep remaining incomplete line
            if let Some(last_line_break) = buffer.rfind('\n') {
                buffer = buffer[last_line_break + 1..].to_string();
            }
        }

        Ok(())
    }

    fn estimate_tokens(&self, request: &CompletionRequest) -> Usage {
        let prompt_tokens = self
            .token_counter
            .count_messages(&request.messages, &self.config.default_model);

        let completion_tokens = request.max_tokens.unwrap_or(2048) as usize;

        Usage {
            prompt_tokens: prompt_tokens as u32,
            completion_tokens: completion_tokens as u32,
            total_tokens: (prompt_tokens + completion_tokens) as u32,
        }
    }

    async fn is_model_available(&self, model: &str) -> bool {
        // Try to fetch from models list
        if let Ok(models) = self.list_models().await {
            return models.iter().any(|m| m.id == model);
        }

        // Fallback to common model patterns
        model.contains('/') || model.contains("-")
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }
}

/// Create an OpenRouter provider configuration
pub fn create_openrouter_config(
    api_key: String,
    model: Option<String>,
    site_url: Option<String>,
    _app_name: Option<String>,
) -> ProviderConfig {
    ProviderConfig {
        name: "openrouter".to_string(),
        provider_type: ProviderType::OpenRouter,
        api_key,
        base_url: None,
        organization: site_url,
        default_model: model.unwrap_or_else(|| "anthropic/claude-3-haiku".to_string()),
        models: vec![
            // Popular OpenRouter models
            "anthropic/claude-3.5-sonnet".to_string(),
            "anthropic/claude-3-opus".to_string(),
            "anthropic/claude-3-haiku".to_string(),
            "openai/gpt-4o".to_string(),
            "openai/gpt-4o-mini".to_string(),
            "openai/gpt-4-turbo".to_string(),
            "google/gemini-pro-1.5".to_string(),
            "meta-llama/llama-3.1-405b-instruct".to_string(),
            "mistralai/mistral-large".to_string(),
        ],
        timeout_ms: 60000,
        max_retries: 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_openrouter_config() {
        let config = create_openrouter_config(
            "test-key".to_string(),
            None,
            Some("https://example.com".to_string()),
            Some("NDC".to_string()),
        );

        assert_eq!(config.name, "openrouter");
        assert_eq!(config.provider_type, ProviderType::OpenRouter);
        assert_eq!(config.default_model, "anthropic/claude-3-haiku");
        assert_eq!(config.organization, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_openrouter_provider_debug() {
        let config = create_openrouter_config("test-key".to_string(), None, None, None);
        let token_counter = Arc::new(SimpleTokenCounter::new());
        let provider = OpenRouterProvider::new(config, token_counter);

        let debug_str = format!("{:?}", provider);
        assert!(debug_str.contains("OpenRouterProvider"));
        assert!(debug_str.contains("openrouter"));
    }

    #[test]
    fn test_openrouter_with_site_info() {
        let config = create_openrouter_config("test-key".to_string(), None, None, None);
        let token_counter = Arc::new(SimpleTokenCounter::new());

        let provider = OpenRouterProvider::with_site_info(
            config,
            token_counter,
            Some("https://example.com".to_string()),
            Some("TestApp".to_string()),
        );

        assert_eq!(provider.site_url, Some("https://example.com".to_string()));
        assert_eq!(provider.app_name, Some("TestApp".to_string()));
    }
}
