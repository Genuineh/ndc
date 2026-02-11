//! Anthropic Provider Implementation
//!
//! Supports:
//! - Claude API (Messages API)
//! - Claude 2, 3, 3.5, 4 series

use super::*;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};

/// Anthropic API version
const ANTHROPIC_API_VERSION: &str = "2023-06-01";

/// Anthropic Provider
#[derive(Clone)]
pub struct AnthropicProvider {
    config: ProviderConfig,
    client: Client,
    token_counter: Arc<dyn TokenCounter>,
}

impl std::fmt::Debug for AnthropicProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicProvider")
            .field("name", &self.config.name)
            .field("default_model", &self.config.default_model)
            .finish_non_exhaustive()
    }
}

impl AnthropicProvider {
    /// Create a new Anthropic provider
    pub fn new(
        config: ProviderConfig,
        token_counter: Arc<dyn TokenCounter>,
    ) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            client,
            token_counter,
        }
    }

    /// Get base URL for API calls
    fn get_base_url(&self) -> String {
        if let Some(url) = &self.config.base_url {
            url.clone()
        } else {
            "https://api.anthropic.com/v1".to_string()
        }
    }

    /// Get auth headers
    fn get_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.config.api_key)
                .parse()
                .unwrap(),
        );
        headers.insert(
            "x-api-key",
            self.config.api_key.parse().unwrap(),
        );
        headers.insert(
            "anthropic-version",
            ANTHROPIC_API_VERSION.parse().unwrap(),
        );
        if let Some(org) = &self.config.organization {
            headers.insert(
                "anthropic-organization",
                org.parse().unwrap(),
            );
        }
        headers
    }

    /// Map model name to Anthropic model ID
    fn map_model_name(&self, model: &str) -> String {
        match model {
            "claude-opus-4" | "opus" | "powerful" => "claude-opus-4-20250514".to_string(),
            "claude-sonnet-4" | "sonnet" | "balanced" => "claude-sonnet-4-20250514".to_string(),
            "claude-haiku-4" | "haiku" | "fast" => "claude-haiku-4-20250514".to_string(),
            "claude-3-5-sonnet" | "claude-3-5" => "claude-sonnet-4-20250514".to_string(),
            "claude-3-opus" => "claude-opus-4-20250514".to_string(),
            "claude-3-haiku" => "claude-haiku-4-20250514".to_string(),
            _ => model.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for AnthropicProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::Anthropic
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        // Anthropic doesn't have a list models endpoint, return configured models
        let models: Vec<ModelInfo> = self.config.models.iter().map(|model_id| {
            ModelInfo {
                id: self.map_model_name(model_id),
                object: "model".to_string(),
                created: 0,
                owned_by: "anthropic".to_string(),
                permission: vec![],
            }
        }).collect();

        Ok(models)
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = format!("{}/messages", self.get_base_url());

        // Check context length
        let estimated = self.estimate_tokens(request);
        if estimated.total_tokens > 200_000 {
            return Err(ProviderError::ContextLengthExceeded {
                length: estimated.total_tokens as usize,
                max_length: 200_000,
            });
        }

        let messages: Vec<serde_json::Value> = request.messages.iter().map(|m| {
            serde_json::json!({
                "role": match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "user", // System handled separately
                    MessageRole::Tool => "user",
                },
                "content": m.content,
            })
        }).collect();

        // Extract system message
        let system = request.messages.iter()
            .find(|m| m.role == MessageRole::System)
            .map(|m| m.content.clone());

        // Build body
        let mut body = serde_json::json!({
            "model": self.map_model_name(&request.model),
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(1024),
            "temperature": request.temperature.unwrap_or(1.0),
        });

        if let Some(sys) = system {
            body["system"] = serde_json::json!(sys);
        }

        if let Some(stop) = &request.stop {
            body["stop_sequences"] = serde_json::json!(stop);
        }

        let response = self
            .client
            .post(&url)
            .headers(self.get_headers())
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network { source: e })?;

        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(ProviderError::Auth {
                message: "Invalid API key".to_string(),
            });
        } else if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited { retry_after: 60 });
        } else if status == StatusCode::BAD_REQUEST {
            let error: serde_json::Value = response.json().await
                .unwrap_or_else(|_| serde_json::json!({}));
            let message = error["error"]["message"]
                .as_str()
                .unwrap_or("Invalid request")
                .to_string();
            return Err(ProviderError::InvalidRequest { message });
        } else if !status.is_success() {
            return Err(ProviderError::Api {
                message: format!("API returned status {}", status),
                status_code: Some(status.as_u16()),
            });
        }

        let data: serde_json::Value = response.json().await
            .map_err(|e| ProviderError::Api {
                message: format!("Failed to parse response: {}", e),
                status_code: None,
            })?;

        // Convert Anthropic response to our format
        let completion = data["content"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        let response = CompletionResponse {
            id: data["id"].as_str().unwrap_or("").to_string(),
            object: "chat.completion".to_string(),
            created: data["created"].as_u64().unwrap_or(0),
            model: data["model"].as_str().unwrap_or(&request.model).to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: completion.to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: data["stop_reason"].as_str().map(|s| s.to_string()),
                logprobs: None,
            }],
            usage: Some(Usage {
                prompt_tokens: data["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
                completion_tokens: data["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
                total_tokens: data["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32
                    + data["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
            }),
        };

        Ok(response)
    }

    async fn complete_streaming(
        &self,
        request: &CompletionRequest,
        handler: &Arc<dyn StreamHandler>,
    ) -> Result<(), ProviderError> {
        let url = format!("{}/messages", self.get_base_url());

        let body = serde_json::json!({
            "model": self.map_model_name(&request.model),
            "messages": request.messages.iter().map(|m| serde_json::json!({
                "role": match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    _ => "user",
                },
                "content": m.content,
            })).collect::<Vec<_>>(),
            "max_tokens": request.max_tokens.unwrap_or(1024),
            "stream": true,
        });

        let mut stream = self
            .client
            .post(&url)
            .headers(self.get_headers())
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network { source: e })?
            .bytes_stream();

        let full_response: Option<CompletionResponse> = None;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ProviderError::Network { source: e })?;
            let text = std::str::from_utf8(&chunk)
                .map_err(|e| ProviderError::Api { message: e.to_string(), status_code: None })?;

            if text.starts_with("data: ") {
                let data = &text[6..];
                if data == "[DONE]" {
                    if let Some(ref response) = full_response {
                        handler.on_complete(response).await?;
                    }
                    return Ok(());
                }

                if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
                    let event_type = value["type"].as_str().unwrap_or("");

                    if event_type == "content_block_delta" {
                        let delta = value["delta"].clone();
                        let content = delta["text"].as_str().unwrap_or("");

                        let chunk = StreamChunk {
                            id: value["id"].as_str().unwrap_or("").to_string(),
                            object: "chat.completion.chunk".to_string(),
                            created: value["created"].as_u64().unwrap_or(0),
                            model: value["model"].as_str().unwrap_or(&request.model).to_string(),
                            choices: vec![StreamChoice {
                                index: 0,
                                delta: Some(Message {
                                    role: MessageRole::Assistant,
                                    content: content.to_string(),
                                    name: None,
                                    tool_calls: None,
                                }),
                                finish_reason: None,
                            }],
                        };

                        handler.on_chunk(&chunk).await?;
                    } else if event_type == "message_stop" {
                        // Message complete
                    }
                }
            }
        }

        Ok(())
    }

    fn estimate_tokens(&self, request: &CompletionRequest) -> Usage {
        let prompt_tokens = self.token_counter.count_messages(&request.messages, &request.model);
        let completion_tokens = request.max_tokens.unwrap_or(1024) as usize;
        Usage {
            prompt_tokens: prompt_tokens as u32,
            completion_tokens: completion_tokens as u32,
            total_tokens: (prompt_tokens + completion_tokens) as u32,
        }
    }

    async fn is_model_available(&self, model: &str) -> bool {
        self.config.models.contains(&model.to_string())
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }
}

/// Create a basic Anthropic configuration
pub fn create_anthropic_config(
    name: &str,
    api_key: &str,
    default_model: &str,
) -> ProviderConfig {
    ProviderConfig {
        name: name.to_string(),
        provider_type: ProviderType::Anthropic,
        api_key: api_key.to_string(),
        base_url: None,
        organization: None,
        default_model: default_model.to_string(),
        models: vec![
            "claude-opus-4".to_string(),
            "claude-sonnet-4".to_string(),
            "claude-haiku-4".to_string(),
            "claude-3-5-sonnet".to_string(),
            "claude-3-opus".to_string(),
        ],
        timeout_ms: 60000,
        max_retries: 3,
    }
}
