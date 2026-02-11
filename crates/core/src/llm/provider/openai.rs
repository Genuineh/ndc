//! OpenAI Provider Implementation
//!
//! Supports:
//! - OpenAI API (Chat Completions)
//! - Azure OpenAI Service
//! - Compatible APIs (Anthropic, local LLMs)

use super::*;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use std::sync::Arc;

/// OpenAI API versions
const _OPENAI_API_VERSION: &str = "2024-02-15-preview";

/// OpenAI Provider
#[derive(Clone)]
pub struct OpenAiProvider {
    config: ProviderConfig,
    client: Client,
    token_counter: Arc<dyn TokenCounter>,
}

impl std::fmt::Debug for OpenAiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiProvider")
            .field("name", &self.config.name)
            .field("default_model", &self.config.default_model)
            .finish_non_exhaustive()
    }
}

impl OpenAiProvider {
    /// Create a new OpenAI provider
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
            "https://api.openai.com/v1".to_string()
        }
    }

    /// Build authorization header
    fn get_auth_header(&self) -> String {
        format!("Bearer {}", self.config.api_key)
    }

    /// Map model name for Azure or compatible APIs
    fn map_model_name(&self, model: &str) -> String {
        model.to_string()
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiProvider {
    fn provider_type(&self) -> ProviderType {
        ProviderType::OpenAi
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let url = format!("{}/models", self.get_base_url());

        let response = self
            .client
            .get(&url)
            .header("Authorization", self.get_auth_header())
            .send()
            .await
            .map_err(|e| ProviderError::Network { source: e })?;

        if !response.status().is_success() {
            return Err(map_provider_error(response.error_for_status().unwrap_err().into(), "openai"));
        }

        let data: serde_json::Value = response.json().await
            .map_err(|e| ProviderError::Api { message: e.to_string(), status_code: None })?;

        let models: Vec<ModelInfo> = serde_json::from_value(data["data"].clone())
            .map_err(|e| ProviderError::Api {
                message: format!("Failed to parse models: {}", e),
                status_code: None,
            })?;

        Ok(models)
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.get_base_url());

        // Check context length
        let estimated = self.estimate_tokens(request);
        if estimated.total_tokens > 128_000 {
            return Err(ProviderError::ContextLengthExceeded {
                length: estimated.total_tokens as usize,
                max_length: 128_000,
            });
        }

        let body = serde_json::json!({
            "model": self.map_model_name(&request.model),
            "messages": request.messages.iter().map(|m| serde_json::json!({
                "role": m.role,
                "content": m.content,
                "name": m.name,
            })).collect::<Vec<_>>(),
            "temperature": request.temperature.unwrap_or(0.7),
            "max_tokens": request.max_tokens,
            "top_p": request.top_p,
            "frequency_penalty": request.frequency_penalty,
            "presence_penalty": request.presence_penalty,
            "stop": request.stop,
            "stream": false,
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", self.get_auth_header())
            .header("Content-Type", "application/json")
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

        let response: CompletionResponse = response.json().await
            .map_err(|e| ProviderError::Api {
                message: format!("Failed to parse response: {}", e),
                status_code: None,
            })?;

        Ok(response)
    }

    async fn complete_streaming(
        &self,
        request: &CompletionRequest,
        handler: &Arc<dyn StreamHandler>,
    ) -> Result<(), ProviderError> {
        let url = format!("{}/chat/completions", self.get_base_url());

        let body = serde_json::json!({
            "model": self.map_model_name(&request.model),
            "messages": request.messages.iter().map(|m| serde_json::json!({
                "role": m.role,
                "content": m.content,
                "name": m.name,
            })).collect::<Vec<_>>(),
            "temperature": request.temperature.unwrap_or(0.7),
            "max_tokens": request.max_tokens,
            "stream": true,
        });

        let mut stream = self
            .client
            .post(&url)
            .header("Authorization", self.get_auth_header())
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network { source: e })?
            .bytes_stream();

        let mut full_response: Option<CompletionResponse> = None;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ProviderError::Network { source: e })?;
            let lines = std::str::from_utf8(&chunk)
                .map_err(|e| ProviderError::Api { message: e.to_string(), status_code: None })?;

            for line in lines.lines() {
                if line.starts_with("data: ") {
                    let data = &line[6..];
                    if data == "[DONE]" {
                        if let Some(ref response) = full_response {
                            handler.on_complete(response).await?;
                        }
                        return Ok(());
                    }

                    if let Ok(stream_chunk) = serde_json::from_str::<StreamChunk>(data) {
                        if full_response.is_none() {
                            full_response = Some(CompletionResponse {
                                id: stream_chunk.id.clone(),
                                object: "chat.completion".to_string(),
                                created: stream_chunk.created,
                                model: stream_chunk.model.clone(),
                                choices: Vec::new(),
                                usage: None,
                            });
                        }

                        handler.on_chunk(&stream_chunk).await?;
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
        // Simple check - in production, verify against model list
        self.config.models.contains(&model.to_string())
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }
}

/// Create a basic OpenAI configuration
pub fn create_openai_config(
    name: &str,
    api_key: &str,
    default_model: &str,
) -> ProviderConfig {
    ProviderConfig {
        name: name.to_string(),
        provider_type: ProviderType::OpenAi,
        api_key: api_key.to_string(),
        base_url: None,
        organization: None,
        default_model: default_model.to_string(),
        models: vec![
            "gpt-4".to_string(),
            "gpt-4-turbo".to_string(),
            "gpt-4o".to_string(),
            "gpt-4o-mini".to_string(),
            "gpt-3.5-turbo".to_string(),
        ],
        timeout_ms: 60000,
        max_retries: 3,
    }
}

/// Create an Azure OpenAI configuration
pub fn create_azure_config(
    name: &str,
    api_key: &str,
    base_url: &str,
    deployment_name: &str,
) -> ProviderConfig {
    ProviderConfig {
        name: name.to_string(),
        provider_type: ProviderType::Azure,
        api_key: api_key.to_string(),
        base_url: Some(format!("{}/openai/deployments/{}", base_url, deployment_name)),
        organization: None,
        default_model: deployment_name.to_string(),
        models: vec![deployment_name.to_string()],
        timeout_ms: 60000,
        max_retries: 3,
    }
}
