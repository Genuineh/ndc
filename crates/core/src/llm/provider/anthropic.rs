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
    pub fn new(config: ProviderConfig, token_counter: Arc<dyn TokenCounter>) -> Self {
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

    /// Parse a string into a header value safely, returning ProviderError on invalid chars.
    fn safe_header_value(s: &str) -> Result<reqwest::header::HeaderValue, ProviderError> {
        s.parse().map_err(|_| ProviderError::InvalidConfig {
            message: "API key or header value contains invalid characters".to_string(),
        })
    }

    /// Get auth headers
    fn get_headers(&self) -> Result<reqwest::header::HeaderMap, ProviderError> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            Self::safe_header_value(&format!("Bearer {}", self.config.api_key))?,
        );
        headers.insert("x-api-key", Self::safe_header_value(&self.config.api_key)?);
        headers.insert(
            "anthropic-version",
            Self::safe_header_value(ANTHROPIC_API_VERSION)?,
        );
        if let Some(org) = &self.config.organization {
            headers.insert("anthropic-organization", Self::safe_header_value(org)?);
        }
        Ok(headers)
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

    fn extract_text_blocks(data: &serde_json::Value) -> String {
        let mut parts: Vec<String> = Vec::new();

        if let Some(content) = data.get("content").and_then(|v| v.as_array()) {
            for block in content {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    if !text.trim().is_empty() {
                        parts.push(text.to_string());
                    }
                    continue;
                }
                if let Some(text) = block.get("content").and_then(|v| v.as_str()) {
                    if !text.trim().is_empty() {
                        parts.push(text.to_string());
                    }
                    continue;
                }
                if let Some(content_parts) = block.get("content").and_then(|v| v.as_array()) {
                    for p in content_parts {
                        if let Some(text) = p.get("text").and_then(|v| v.as_str())
                            && !text.trim().is_empty()
                        {
                            parts.push(text.to_string());
                        }
                    }
                }
            }
        }

        if parts.is_empty()
            && let Some(text) = data.get("output_text").and_then(|v| v.as_str())
            && !text.trim().is_empty()
        {
            parts.push(text.to_string());
        }

        parts.join("\n")
    }

    fn extract_tool_calls(data: &serde_json::Value) -> Vec<ToolCall> {
        data.get("content")
            .and_then(|v| v.as_array())
            .map(|content| {
                content
                    .iter()
                    .filter_map(|block| {
                        let block_type = block.get("type").and_then(|v| v.as_str())?;
                        if block_type != "tool_use" {
                            return None;
                        }

                        let id = block
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("tool-use")
                            .to_string();
                        let name = block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown_tool")
                            .to_string();
                        let arguments = block
                            .get("input")
                            .map(|v| {
                                if v.is_string() {
                                    v.as_str().unwrap_or("{}").to_string()
                                } else {
                                    v.to_string()
                                }
                            })
                            .unwrap_or_else(|| "{}".to_string());

                        Some(ToolCall {
                            id,
                            function: ToolCallFunction { name, arguments },
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    fn map_openai_tool_to_anthropic(tool: &serde_json::Value) -> Option<serde_json::Value> {
        let function = tool.get("function")?;
        let name = function.get("name")?.as_str()?;
        let description = function
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let input_schema = function
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"type":"object","properties":{}}));

        Some(serde_json::json!({
            "name": name,
            "description": description,
            "input_schema": input_schema,
        }))
    }

    fn serialize_messages_for_anthropic(
        &self,
        request: &CompletionRequest,
    ) -> Vec<serde_json::Value> {
        request
            .messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .map(|m| match m.role {
                MessageRole::Assistant => {
                    let mut blocks: Vec<serde_json::Value> = Vec::new();

                    if !m.content.trim().is_empty() {
                        blocks.push(serde_json::json!({
                            "type": "text",
                            "text": m.content,
                        }));
                    }

                    if let Some(tool_calls) = &m.tool_calls {
                        for tc in tool_calls {
                            let parsed_input =
                                serde_json::from_str::<serde_json::Value>(&tc.function.arguments)
                                    .unwrap_or_else(|_| serde_json::json!({}));
                            blocks.push(serde_json::json!({
                                "type": "tool_use",
                                "id": tc.id,
                                "name": tc.function.name,
                                "input": parsed_input,
                            }));
                        }
                    }

                    serde_json::json!({
                        "role": "assistant",
                        "content": if blocks.is_empty() {
                            serde_json::json!("")
                        } else {
                            serde_json::Value::Array(blocks)
                        },
                    })
                }
                MessageRole::Tool => {
                    let tool_use_id = match m.name.clone() {
                        Some(id) if !id.is_empty() => id,
                        _ => {
                            tracing::warn!(
                                "Skipping Tool message with missing tool_use_id in Anthropic serialization"
                            );
                            // 返回空 user 消息以保持消息序列，避免发送非法 tool_result
                            return serde_json::json!({
                                "role": "user",
                                "content": m.content,
                            });
                        }
                    };
                    serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_use_id,
                            "content": m.content,
                            "is_error": false,
                        }],
                    })
                }
                MessageRole::User => serde_json::json!({
                    "role": "user",
                    "content": m.content,
                }),
                MessageRole::System => unreachable!(),
            })
            .collect()
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
        let models: Vec<ModelInfo> = self
            .config
            .models
            .iter()
            .map(|model_id| ModelInfo {
                id: self.map_model_name(model_id),
                object: "model".to_string(),
                created: 0,
                owned_by: "anthropic".to_string(),
                permission: vec![],
            })
            .collect();

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

        let messages = self.serialize_messages_for_anthropic(request);

        // Extract system message
        let system = request
            .messages
            .iter()
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

        if let Some(tools) = request.tools.as_ref().filter(|t| !t.is_empty()) {
            let mapped_tools: Vec<serde_json::Value> = tools
                .iter()
                .filter_map(Self::map_openai_tool_to_anthropic)
                .collect();
            if !mapped_tools.is_empty() {
                body["tools"] = serde_json::json!(mapped_tools);
            }
        }

        let response = self
            .client
            .post(&url)
            .headers(self.get_headers()?)
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
            let error: serde_json::Value = response
                .json()
                .await
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

        let data: serde_json::Value = response.json().await.map_err(|e| ProviderError::Api {
            message: format!("Failed to parse response: {}", e),
            status_code: None,
        })?;

        let completion = Self::extract_text_blocks(&data);
        let tool_calls = Self::extract_tool_calls(&data);
        if completion.trim().is_empty() && tool_calls.is_empty() {
            let stop_reason = data
                .get("stop_reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let keys = data
                .as_object()
                .map(|obj| obj.keys().cloned().collect::<Vec<_>>().join(","))
                .unwrap_or_else(|| "<non-object>".to_string());
            return Err(ProviderError::Api {
                message: format!(
                    "Anthropic-compatible response has empty text content (stop_reason={}, keys={})",
                    stop_reason, keys
                ),
                status_code: None,
            });
        }

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
                    tool_calls: if tool_calls.is_empty() {
                        None
                    } else {
                        Some(tool_calls)
                    },
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
            .headers(self.get_headers()?)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network { source: e })?
            .bytes_stream();

        let full_response: Option<CompletionResponse> = None;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ProviderError::Network { source: e })?;
            let text = std::str::from_utf8(&chunk).map_err(|e| ProviderError::Api {
                message: e.to_string(),
                status_code: None,
            })?;

            if let Some(data) = text.strip_prefix("data: ") {
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
                            model: value["model"]
                                .as_str()
                                .unwrap_or(&request.model)
                                .to_string(),
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
        let prompt_tokens = self
            .token_counter
            .count_messages(&request.messages, &request.model);
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
pub fn create_anthropic_config(name: &str, api_key: &str, default_model: &str) -> ProviderConfig {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_blocks_prefers_all_text_blocks() {
        let data = serde_json::json!({
            "content": [
                { "type": "thinking", "thinking": "hidden" },
                { "type": "text", "text": "hello" },
                { "type": "text", "text": "world" }
            ]
        });
        let out = AnthropicProvider::extract_text_blocks(&data);
        assert_eq!(out, "hello\nworld");
    }

    #[test]
    fn test_extract_text_blocks_with_nested_content_parts() {
        let data = serde_json::json!({
            "content": [
                {
                    "type": "message",
                    "content": [
                        { "type": "text", "text": "nested-text" }
                    ]
                }
            ]
        });
        let out = AnthropicProvider::extract_text_blocks(&data);
        assert_eq!(out, "nested-text");
    }

    #[test]
    fn test_extract_tool_calls_from_content_blocks() {
        let data = serde_json::json!({
            "content": [
                {"type":"text","text":"planning"},
                {
                    "type":"tool_use",
                    "id":"toolu_1",
                    "name":"list",
                    "input":{"path":"."}
                }
            ]
        });

        let calls = AnthropicProvider::extract_tool_calls(&data);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "toolu_1");
        assert_eq!(calls[0].function.name, "list");
        assert_eq!(calls[0].function.arguments, r#"{"path":"."}"#);
    }

    #[test]
    fn test_map_openai_tool_to_anthropic() {
        let openai_tool = serde_json::json!({
            "type":"function",
            "function":{
                "name":"file_read",
                "description":"Read file",
                "parameters":{"type":"object","properties":{"path":{"type":"string"}}}
            }
        });

        let mapped = AnthropicProvider::map_openai_tool_to_anthropic(&openai_tool).unwrap();
        assert_eq!(mapped["name"], "file_read");
        assert_eq!(mapped["description"], "Read file");
        assert_eq!(mapped["input_schema"]["type"], "object");
    }

    #[test]
    fn test_serialize_messages_includes_tool_result_block() {
        let config = create_anthropic_config("anthropic", "test-key", "claude-sonnet-4");
        let provider = AnthropicProvider::new(config, Arc::new(SimpleTokenCounter::new()));
        let request = CompletionRequest {
            model: "claude-sonnet-4".to_string(),
            messages: vec![
                Message {
                    role: MessageRole::System,
                    content: "system".to_string(),
                    name: None,
                    tool_calls: None,
                },
                Message {
                    role: MessageRole::Assistant,
                    content: String::new(),
                    name: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "toolu_1".to_string(),
                        function: ToolCallFunction {
                            name: "list".to_string(),
                            arguments: r#"{"path":"."}"#.to_string(),
                        },
                    }]),
                },
                Message {
                    role: MessageRole::Tool,
                    content: "[]".to_string(),
                    name: Some("toolu_1".to_string()),
                    tool_calls: None,
                },
            ],
            temperature: Some(0.1),
            max_tokens: Some(256),
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            stream: false,
            tools: None,
        };

        let mapped = provider.serialize_messages_for_anthropic(&request);
        assert_eq!(mapped.len(), 2);
        assert_eq!(mapped[0]["role"], "assistant");
        assert_eq!(mapped[0]["content"][0]["type"], "tool_use");
        assert_eq!(mapped[1]["role"], "user");
        assert_eq!(mapped[1]["content"][0]["type"], "tool_result");
        assert_eq!(mapped[1]["content"][0]["tool_use_id"], "toolu_1");
    }

    #[test]
    fn test_get_headers_returns_error_on_invalid_api_key() {
        // API key with newline characters should NOT panic — must return InvalidConfig error
        let config = create_anthropic_config("anthropic", "bad\nkey\r\n", "claude-sonnet-4");
        let provider = AnthropicProvider::new(config, Arc::new(SimpleTokenCounter::new()));
        let result = provider.get_headers();
        assert!(result.is_err(), "Expected Err for API key with control chars");
        let err = result.unwrap_err();
        assert!(
            matches!(err, ProviderError::InvalidConfig { .. }),
            "Expected InvalidConfig, got: {err:?}"
        );
    }

    #[test]
    fn test_get_headers_succeeds_with_valid_key() {
        let config = create_anthropic_config("anthropic", "sk-ant-valid-key-1234", "claude-sonnet-4");
        let provider = AnthropicProvider::new(config, Arc::new(SimpleTokenCounter::new()));
        let result = provider.get_headers();
        assert!(result.is_ok(), "Valid API key should succeed: {result:?}");
    }

    #[test]
    fn test_provider_config_debug_masks_api_key() {
        let config = create_anthropic_config("anthropic", "sk-ant-secret-key-12345678", "claude-sonnet-4");
        let debug_output = format!("{:?}", config);
        assert!(
            !debug_output.contains("sk-ant-secret-key-12345678"),
            "Debug output must NOT contain full API key: {debug_output}"
        );
        assert!(
            debug_output.contains("sk-a***"),
            "Debug output should contain masked prefix: {debug_output}"
        );
    }
}
