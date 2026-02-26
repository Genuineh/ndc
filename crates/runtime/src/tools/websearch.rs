//! WebSearch Tool - Web search functionality
//!
//! Responsibilities:
//! - Search the web using search APIs
//! - Parse search results
//! - Return formatted results

use super::{Tool, ToolError, ToolResult};
use std::time::Duration;
use tracing::debug;

/// WebSearch tool
#[derive(Debug)]
pub struct WebSearchTool {
    /// Request timeout
    timeout_seconds: u64,
    /// Maximum results
    max_results: usize,
    /// Search provider
    provider: String,
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self {
            timeout_seconds: 30,
            max_results: 10,
            provider: "duckduckgo".to_string(),
        }
    }

    /// Perform search
    async fn search(&self, query: &str) -> Result<String, ToolError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_seconds))
            .build()
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        // DuckDuckGo Instant Answer API (free, no API key required)
        let url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
            urlencoding::encode(query)
        );

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Search request failed: {}", e)))?;

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Parse response failed: {}", e)))?;

        // Format results
        let mut output = format!("Search results for: \"{}\"\n\n", query);

        if let Some(abstract_summary) = json.get("Abstract").and_then(|v| v.as_str())
            && !abstract_summary.is_empty()
        {
            output.push_str(&format!("Summary:\n{}\n\n", abstract_summary));
        }

        if let Some(results) = json.get("RelatedTopics").and_then(|v| v.as_array()) {
            let mut count = 0;
            for item in results {
                if count >= self.max_results {
                    break;
                }

                if let Some(text) = item.get("Text").and_then(|v| v.as_str())
                    && let Some(url) = item.get("FirstURL").and_then(|v| v.as_str())
                {
                    output.push_str(&format!("{}. {}\n   {}\n\n", count + 1, text, url));
                    count += 1;
                }
            }

            if count == 0 {
                output.push_str("No results found.\n");
            }
        } else if json.get("Answer").is_none() && json.get("Abstract").is_none() {
            output.push_str("No results found.\n");
        }

        Ok(output)
    }
}

#[async_trait::async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "websearch"
    }

    fn description(&self) -> &str {
        "Search the web using DuckDuckGo"
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing query".to_string()))?;

        let max_results = params
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.max_results as u64) as usize;

        debug!("WebSearch: {}", query);

        let start = std::time::Instant::now();

        // Create a custom searcher with the requested max_results
        let mut searcher = self.clone();
        searcher.max_results = max_results;

        let output = searcher.search(query).await?;
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
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "max_results": {
                    "type": "number",
                    "description": "Maximum number of results (default: 10)"
                }
            },
            "required": ["query"]
        })
    }
}

impl Clone for WebSearchTool {
    fn clone(&self) -> Self {
        Self {
            timeout_seconds: self.timeout_seconds,
            max_results: self.max_results,
            provider: self.provider.clone(),
        }
    }
}
