//! WebFetch Tool - HTTP content retrieval
//!
//! Responsibilities:
//! - Fetch HTTP/HTTPS content
//! - Handle redirects
//! - Support headers and methods
//! - Parse response status

use super::{Tool, ToolResult, ToolError};
use std::time::Duration;
use tracing::debug;

/// WebFetch tool
#[derive(Debug)]
pub struct WebFetchTool {
    /// Request timeout
    timeout_seconds: u64,
    /// Maximum content size (bytes)
    max_content_size: usize,
    /// Follow redirects
    #[allow(dead_code)]
    _follow_redirects: bool,
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            timeout_seconds: 30,
            max_content_size: 1024 * 1024, // 1MB
            _follow_redirects: true,
        }
    }

    /// Fetch URL content
    async fn fetch(&self, url: &str, method: &str, headers: Option<&serde_json::Value>, body: Option<&str>) -> Result<String, ToolError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_seconds))
            .build()
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let mut request = match method {
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "HEAD" => client.head(url),
            _ => client.get(url),
        };

        // Add headers
        if let Some(h) = headers {
            for (key, value) in h.as_object().unwrap_or(&serde_json::Map::new()) {
                if let Some(v) = value.as_str() {
                    request = request.header(key, v);
                }
            }
        }

        // Add body for POST/PUT
        if let Some(b) = body {
            request = request.body(b.to_string());
        }

        let response = request.send().await
            .map_err(|e| ToolError::ExecutionFailed(format!("Request failed: {}", e)))?;

        let status = response.status();
        let text = response.text().await
            .map_err(|e| ToolError::ExecutionFailed(format!("Read response failed: {}", e)))?;

        // Check content size
        if text.len() > self.max_content_size {
            return Ok(format!(
                "[Content truncated - {} bytes]\n\n{}...",
                text.len(),
                &text[..std::cmp::min(1024, text.len())]
            ));
        }

        Ok(format!("Status: {}\n\n{}", status, text))
    }
}

#[async_trait::async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "webfetch"
    }

    fn description(&self) -> &str {
        "Fetch content from a URL (HTTP GET, POST, PUT, DELETE)"
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let url = params.get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing URL".to_string()))?;

        let method = params.get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET");

        let headers = params.get("headers").and_then(|v| Some(v));

        let body = params.get("body")
            .and_then(|v| v.as_str());

        debug!("WebFetch: {} {}", method, url);

        let start = std::time::Instant::now();

        let output = self.fetch(url, method, headers, body).await?;
        let duration = start.elapsed().as_millis() as u64;
        let bytes = output.len();

        Ok(ToolResult {
            success: true,
            output,
            error: None,
            metadata: super::ToolMetadata {
                execution_time_ms: duration,
                files_read: 0,
                files_written: 0,
                bytes_processed: bytes as u64,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE", "HEAD"],
                    "description": "HTTP method (default: GET)"
                },
                "headers": {
                    "type": "object",
                    "description": "HTTP headers as key-value pairs"
                },
                "body": {
                    "type": "string",
                    "description": "Request body for POST/PUT"
                }
            },
            "required": ["url"]
        })
    }
}
