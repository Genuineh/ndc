//! MiniMax Provider Implementation
//!
//! Supports:
//! - MiniMax AI API (Chat Completions)
//! - Streaming responses
//! - Multiple model types (M2.1, abab6.5s, abab6.5, etc.)
//!
//! API Documentation: https://api.minimax.chat/

use super::*;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use std::sync::Arc;

/// MiniMax API base URL
const MINIMAX_BASE_URL: &str = "https://api.minimax.chat/v1";

/// MiniMax Provider
#[derive(Clone)]
pub struct MiniMaxProvider {
    config: ProviderConfig,
    client: Client,
    token_counter: Arc<dyn TokenCounter>,
    group_id: Option<String>,
}

impl std::fmt::Debug for MiniMaxProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MiniMaxProvider")
            .field("name", &self.config.name)
            .field("default_model", &self.config.default_model)
            .field("group_id", &self.group_id)
            .finish_non_exhaustive()
    }
}

impl MiniMaxProvider {
    /// Create a new MiniMax provider
    pub fn new(config: ProviderConfig, token_counter: Arc<dyn TokenCounter>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .expect("Failed to create HTTP client");

        // Extract group_id from organization field if provided
        let group_id = config.organization.clone();

        Self {
            config,
            client,
            token_counter,
            group_id,
        }
    }

    /// Create MiniMax provider with group_id
    pub fn with_group_id(
        config: ProviderConfig,
        token_counter: Arc<dyn TokenCounter>,
        group_id: String,
    ) -> Self {
        let mut provider = Self::new(config, token_counter);
        provider.group_id = Some(group_id);
        provider
    }

    /// Get base URL for API calls
    fn get_base_url(&self) -> String {
        if let Some(url) = &self.config.base_url {
            url.clone()
        } else {
            MINIMAX_BASE_URL.to_string()
        }
    }

    /// Build authorization header
    fn get_auth_header(&self) -> String {
        format!("Bearer {}", self.config.api_key)
    }

    /// Map model name for MiniMax API
    fn map_model_name(&self, model: &str) -> String {
        // MiniMax model names mapping
        match model {
            "gpt-4" | "gpt-3.5-turbo" => "abab6.5s-chat".to_string(),
            "claude-3-opus" | "claude-3-sonnet" => "abab6.5s-chat".to_string(),
            _ => model.to_string(),
        }
    }

    /// Build request body for MiniMax API
    fn build_request_body(&self, request: &CompletionRequest) -> serde_json::Value {
        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|m| {
                // MiniMax chatcompletion_v2 expects OpenAI-like messages.
                // Keep roles strictly in {system,user,assistant}.
                let role = self.map_role_to_sender(match m.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => "tool",
                });
                let name = m.name.clone().unwrap_or_else(|| match role.as_str() {
                    "assistant" => "MiniMax AI".to_string(),
                    _ => "user".to_string(),
                });

                let mut msg = serde_json::json!({
                    "role": role,
                    "name": name,
                });

                if !m.content.trim().is_empty() {
                    msg["content"] = serde_json::json!(m.content);
                }

                msg
            })
            .collect();

        serde_json::json!({
            "model": self.map_model_name(&request.model),
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.9),
            "top_p": request.top_p.unwrap_or(0.95),
            "max_tokens": request.max_tokens,
            "tokens_to_generate": request.max_tokens,
            "stream": false,
        })
    }

    /// Map role to MiniMax sender_type
    fn map_role_to_sender(&self, role: &str) -> String {
        match role {
            "system" => "system".to_string(),
            "user" => "user".to_string(),
            "assistant" => "assistant".to_string(),
            "tool" => "user".to_string(),
            _ => "user".to_string(),
        }
    }

    fn extract_text_from_value(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            serde_json::Value::Array(items) => items.iter().find_map(Self::extract_text_from_value),
            serde_json::Value::Object(map) => {
                for key in ["text", "reply", "answer", "output_text"] {
                    if let Some(v) = map.get(key)
                        && let Some(text) = Self::extract_text_from_value(v)
                    {
                        return Some(text);
                    }
                }

                for key in ["content", "message", "messages", "delta"] {
                    if let Some(v) = map.get(key)
                        && let Some(text) = Self::extract_text_from_value(v)
                    {
                        return Some(text);
                    }
                }

                None
            }
            _ => None,
        }
    }

    /// Parse MiniMax response to CompletionResponse
    fn parse_response(
        &self,
        response_value: serde_json::Value,
    ) -> Result<CompletionResponse, ProviderError> {
        if let Some(base_resp) = response_value.get("base_resp") {
            let status_code = base_resp
                .get("status_code")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if status_code != 0 {
                let status_msg = base_resp
                    .get("status_msg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown minimax error");
                return Err(ProviderError::Api {
                    message: format!("MiniMax base_resp error {}: {}", status_code, status_msg),
                    status_code: None,
                });
            }
        }

        let first_choice = response_value
            .get("choices")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first());

        let text = first_choice
            .and_then(Self::extract_text_from_value)
            .or_else(|| Self::extract_text_from_value(&response_value))
            .ok_or_else(|| {
                let keys = response_value
                    .as_object()
                    .map(|obj| obj.keys().cloned().collect::<Vec<_>>().join(","))
                    .unwrap_or_else(|| "<non-object>".to_string());
                ProviderError::Api {
                    message: format!(
                        "Missing text content in MiniMax response (top-level keys: {})",
                        keys
                    ),
                    status_code: None,
                }
            })?;

        // Parse usage if available
        let usage = response_value.get("usage").map(|u| Usage {
            prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
        });

        let finish_reason = first_choice
            .and_then(|choice| choice.get("finish_reason"))
            .or_else(|| first_choice.and_then(|choice| choice.get("finishReason")))
            .and_then(|v| v.as_str())
            .unwrap_or("stop")
            .to_string();

        Ok(CompletionResponse {
            id: response_value["id"]
                .as_str()
                .or_else(|| response_value["request_id"].as_str())
                .unwrap_or("minimax-unknown")
                .to_string(),
            object: "chat.completion".to_string(),
            created: response_value["created"].as_u64().unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            }),
            model: self.config.default_model.clone(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: MessageRole::Assistant,
                    content: text.to_string(),
                    name: None,
                    tool_calls: None,
                },
                finish_reason: Some(finish_reason),
                logprobs: None,
            }],
            usage,
        })
    }
}

#[async_trait::async_trait]
impl LlmProvider for MiniMaxProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::MiniMax
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        // Try to fetch models from API first
        let url = format!("{}/models", self.get_base_url());

        let mut req_builder = self
            .client
            .get(&url)
            .header("Authorization", self.get_auth_header());

        // Add group_id header if available
        if let Some(ref group_id) = self.group_id {
            req_builder = req_builder.header("GroupId", group_id);
        }

        let response = req_builder.send().await;

        if let Ok(resp) = response
            && resp.status().is_success()
            && let Ok(data) = resp.json::<serde_json::Value>().await
            && let Some(models_array) = data.get("data").and_then(|v| v.as_array())
        {
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
                            .unwrap_or("minimax")
                            .to_string(),
                        permission: vec![],
                    })
                })
                .collect();

            if !models.is_empty() {
                return Ok(models);
            }
        }

        // Fallback to static model list
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let models = vec![
            ModelInfo {
                id: "abab6.5s-chat".to_string(),
                object: "model".to_string(),
                created: now,
                owned_by: "minimax".to_string(),
                permission: vec![],
            },
            ModelInfo {
                id: "abab6.5-chat".to_string(),
                object: "model".to_string(),
                created: now,
                owned_by: "minimax".to_string(),
                permission: vec![],
            },
        ];

        Ok(models)
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = format!("{}/text/chatcompletion_v2", self.get_base_url());

        let body = self.build_request_body(request);

        let mut req_builder = self
            .client
            .post(&url)
            .header("Authorization", self.get_auth_header())
            .header("Content-Type", "application/json");

        // Add group_id header if available
        if let Some(ref group_id) = self.group_id {
            req_builder = req_builder.header("GroupId", group_id);
            req_builder = req_builder.query(&[("GroupId", group_id)]);
        }

        let response = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network { source: e })?;

        let status = response.status();

        if status == StatusCode::UNAUTHORIZED {
            return Err(ProviderError::Auth {
                message: "Invalid MiniMax API key".to_string(),
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

            if error_text.contains("context_length_exceeded") || error_text.contains("token") {
                return Err(ProviderError::ContextLengthExceeded {
                    length: self.estimate_tokens(request).total_tokens as usize,
                    max_length: 8192,
                });
            }

            return Err(ProviderError::Api {
                message: format!("MiniMax API error: {}", error_text),
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
        let url = format!("{}/text/chatcompletion_v2", self.get_base_url());

        let mut body = self.build_request_body(request);

        // Enable streaming
        body["stream"] = serde_json::json!(true);

        let mut req_builder = self
            .client
            .post(&url)
            .header("Authorization", self.get_auth_header())
            .header("Content-Type", "application/json");

        // Add group_id header if available
        if let Some(ref group_id) = self.group_id {
            req_builder = req_builder.header("GroupId", group_id);
            req_builder = req_builder.query(&[("GroupId", group_id)]);
        }

        let response = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network { source: e })?;

        let status = response.status();

        if status == StatusCode::UNAUTHORIZED {
            return Err(ProviderError::Auth {
                message: "Invalid MiniMax API key".to_string(),
            });
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ProviderError::Api {
                message: format!("MiniMax API error: {}", error_text),
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

            // MiniMax SSE format: data: {...}
            for line in buffer.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        if let Some(ref response) = full_response {
                            handler.on_complete(response).await?;
                        }
                        return Ok(());
                    }

                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
                        let chunk_text = value["choices"]
                            .get(0)
                            .and_then(|c| c.get("text"))
                            .and_then(|t| t.as_str())
                            .or_else(|| {
                                value["choices"]
                                    .get(0)
                                    .and_then(|c| c.get("delta"))
                                    .and_then(|d| d.get("content"))
                                    .and_then(|v| v.as_str())
                            })
                            .or_else(|| value.get("reply").and_then(|v| v.as_str()))
                            .unwrap_or("");

                        if !chunk_text.is_empty() {
                            let stream_chunk = StreamChunk {
                                id: value["id"].as_str().unwrap_or("").to_string(),
                                object: "chat.completion.chunk".to_string(),
                                created: value["created"].as_u64().unwrap_or(0),
                                model: self.config.default_model.clone(),
                                choices: vec![StreamChoice {
                                    index: 0,
                                    delta: Some(Message {
                                        role: MessageRole::Assistant,
                                        content: chunk_text.to_string(),
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
                                    object: "chat.completion".to_string(),
                                    created: value["created"].as_u64().unwrap_or(0),
                                    model: self.config.default_model.clone(),
                                    choices: vec![Choice {
                                        index: 0,
                                        message: Message {
                                            role: MessageRole::Assistant,
                                            content: chunk_text.to_string(),
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
                                    choice.message.content.push_str(chunk_text);
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
        // Check if model is in the supported list
        let supported_models = ["abab6.5s-chat", "abab6.5-chat", "abab5.5-chat"];

        let mapped_model = self.map_model_name(model);
        supported_models.contains(&mapped_model.as_str())
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }
}

/// Create a MiniMax provider configuration
pub fn create_minimax_config(
    api_key: String,
    group_id: Option<String>,
    model: Option<String>,
) -> ProviderConfig {
    ProviderConfig {
        name: "minimax".to_string(),
        provider_type: ProviderType::MiniMax,
        api_key,
        base_url: None,
        organization: group_id,
        default_model: model.unwrap_or_else(|| "abab6.5s-chat".to_string()),
        models: vec![
            "abab6.5s-chat".to_string(),
            "abab6.5-chat".to_string(),
            "abab5.5-chat".to_string(),
        ],
        timeout_ms: 60000,
        max_retries: 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_minimax_config() {
        let config =
            create_minimax_config("test-key".to_string(), Some("test-group".to_string()), None);

        assert_eq!(config.name, "minimax");
        assert_eq!(config.provider_type, ProviderType::MiniMax);
        assert_eq!(config.default_model, "abab6.5s-chat");
        assert_eq!(config.organization, Some("test-group".to_string()));
    }

    #[test]
    fn test_map_model_name() {
        let config = create_minimax_config("test-key".to_string(), None, None);
        let token_counter = Arc::new(SimpleTokenCounter::new());
        let provider = MiniMaxProvider::new(config, token_counter);

        assert_eq!(provider.map_model_name("gpt-4"), "abab6.5s-chat");
        assert_eq!(provider.map_model_name("abab6.5s-chat"), "abab6.5s-chat");
    }

    #[test]
    fn test_map_role_to_sender() {
        let config = create_minimax_config("test-key".to_string(), None, None);
        let token_counter = Arc::new(SimpleTokenCounter::new());
        let provider = MiniMaxProvider::new(config, token_counter);

        assert_eq!(provider.map_role_to_sender("user"), "user");
        assert_eq!(provider.map_role_to_sender("system"), "system");
        assert_eq!(provider.map_role_to_sender("assistant"), "assistant");
        assert_eq!(provider.map_role_to_sender("tool"), "user");
    }

    #[test]
    fn test_minimax_provider_debug() {
        let config =
            create_minimax_config("test-key".to_string(), Some("test-group".to_string()), None);
        let token_counter = Arc::new(SimpleTokenCounter::new());
        let provider = MiniMaxProvider::new(config, token_counter);

        let debug_str = format!("{:?}", provider);
        assert!(debug_str.contains("MiniMaxProvider"));
        assert!(debug_str.contains("minimax"));
    }

    #[test]
    fn test_parse_response_with_reply_shape() {
        let config = create_minimax_config("test-key".to_string(), None, None);
        let token_counter = Arc::new(SimpleTokenCounter::new());
        let provider = MiniMaxProvider::new(config, token_counter);

        let payload = serde_json::json!({
            "id": "resp-1",
            "reply": "你好，这是 MiniMax",
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });

        let response = provider.parse_response(payload).expect("should parse");
        assert_eq!(response.id, "resp-1");
        assert_eq!(response.choices[0].message.content, "你好，这是 MiniMax");
        assert_eq!(response.usage.unwrap().total_tokens, 15);
    }

    #[test]
    fn test_parse_response_with_message_content_shape() {
        let config = create_minimax_config("test-key".to_string(), None, None);
        let token_counter = Arc::new(SimpleTokenCounter::new());
        let provider = MiniMaxProvider::new(config, token_counter);

        let payload = serde_json::json!({
            "id": "resp-2",
            "created": 123,
            "choices": [
                {
                    "message": {
                        "content": "message-content"
                    },
                    "finish_reason": "stop"
                }
            ]
        });

        let response = provider.parse_response(payload).expect("should parse");
        assert_eq!(response.id, "resp-2");
        assert_eq!(response.created, 123);
        assert_eq!(response.choices[0].message.content, "message-content");
    }

    #[test]
    fn test_parse_response_with_content_parts_array() {
        let config = create_minimax_config("test-key".to_string(), None, None);
        let token_counter = Arc::new(SimpleTokenCounter::new());
        let provider = MiniMaxProvider::new(config, token_counter);

        let payload = serde_json::json!({
            "id": "resp-3",
            "choices": [
                {
                    "message": {
                        "content": [
                            { "type": "text", "text": "part-text-content" }
                        ]
                    }
                }
            ]
        });

        let response = provider.parse_response(payload).expect("should parse");
        assert_eq!(response.choices[0].message.content, "part-text-content");
    }

    #[test]
    fn test_parse_response_with_base_resp_error() {
        let config = create_minimax_config("test-key".to_string(), None, None);
        let token_counter = Arc::new(SimpleTokenCounter::new());
        let provider = MiniMaxProvider::new(config, token_counter);

        let payload = serde_json::json!({
            "base_resp": {
                "status_code": 1004,
                "status_msg": "invalid group id"
            },
            "choices": [],
            "usage": {}
        });

        let err = provider.parse_response(payload).expect_err("should fail");
        let err_text = err.to_string();
        assert!(err_text.contains("MiniMax base_resp error 1004"));
        assert!(err_text.contains("invalid group id"));
    }

    #[test]
    fn test_build_request_body_normalizes_system_role() {
        let config = create_minimax_config("test-key".to_string(), None, None);
        let token_counter = Arc::new(SimpleTokenCounter::new());
        let provider = MiniMaxProvider::new(config, token_counter);

        let request = CompletionRequest {
            model: "abab6.5s-chat".to_string(),
            messages: vec![
                Message {
                    role: MessageRole::System,
                    content: "you are helpful".to_string(),
                    name: None,
                    tool_calls: None,
                },
                Message {
                    role: MessageRole::User,
                    content: "hello".to_string(),
                    name: None,
                    tool_calls: None,
                },
            ],
            temperature: Some(0.3),
            max_tokens: Some(256),
            top_p: Some(0.9),
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            stream: false,
            tools: None,
        };

        let body = provider.build_request_body(&request);
        let messages = body["messages"].as_array().unwrap();
        assert!(!messages.is_empty());
        assert_eq!(messages[0]["role"].as_str(), Some("system"));
        let text = messages[0]["content"].as_str().unwrap_or("");
        assert!(text.contains("you are helpful"));
        assert_eq!(messages[1]["role"].as_str(), Some("user"));
    }
}
