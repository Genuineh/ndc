//! WebFetch Tool - HTTP content retrieval
//!
//! Responsibilities:
//! - Fetch HTTP/HTTPS content
//! - Handle redirects
//! - Support headers and methods
//! - Parse response status

use super::{Tool, ToolError, ToolResult};
use std::net::IpAddr;
use std::time::Duration;
use tracing::debug;

/// Validate that a URL is safe to fetch (SSRF protection).
///
/// Rejects:
/// - Non HTTP/HTTPS schemes (file://, ftp://, gopher://, etc.)
/// - Private / loopback / link-local IP addresses
/// - Cloud metadata endpoints (169.254.169.254)
fn validate_url_safety(url_str: &str) -> Result<(), ToolError> {
    let parsed = url::Url::parse(url_str)
        .map_err(|e| ToolError::InvalidArgument(format!("Invalid URL: {e}")))?;

    // Scheme whitelist
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(ToolError::InvalidArgument(format!(
                "URL scheme '{scheme}' not allowed; only http/https permitted"
            )));
        }
    }

    // Resolve hostname and check for private IPs
    if let Some(host) = parsed.host_str() {
        // Try to parse as IP directly
        if let Ok(ip) = host.parse::<IpAddr>() {
            if is_private_ip(&ip) {
                return Err(ToolError::InvalidArgument(format!(
                    "URL targets private/reserved IP address: {ip}"
                )));
            }
        }
        // Block well-known dangerous hostnames
        let lower = host.to_ascii_lowercase();
        if lower == "localhost"
            || lower == "metadata.google.internal"
            || lower.ends_with(".internal")
        {
            return Err(ToolError::InvalidArgument(format!(
                "URL targets blocked hostname: {host}"
            )));
        }
    } else {
        return Err(ToolError::InvalidArgument("URL has no host".to_string()));
    }

    Ok(())
}

/// Check if an IP address belongs to a private, loopback, or link-local range.
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()           // 127.0.0.0/8
                || v4.is_private()     // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || v4.is_link_local()  // 169.254.0.0/16 (AWS metadata, etc.)
                || v4.is_unspecified() // 0.0.0.0
                || v4.is_broadcast() // 255.255.255.255
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()           // ::1
                || v6.is_unspecified() // ::
        }
    }
}

/// WebFetch tool
#[derive(Debug)]
pub struct WebFetchTool {
    /// Request timeout
    timeout_seconds: u64,
    /// Maximum content size (bytes)
    max_content_size: usize,
    /// Follow redirects
    _follow_redirects: bool,
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
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
    async fn fetch(
        &self,
        url: &str,
        method: &str,
        headers: Option<&serde_json::Value>,
        body: Option<&str>,
    ) -> Result<String, ToolError> {
        validate_url_safety(url)?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_seconds))
            .redirect(reqwest::redirect::Policy::none())
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

        let response = request
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Request failed: {}", e)))?;

        let status = response.status();
        let text = response
            .text()
            .await
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
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing URL".to_string()))?;

        let method = params
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET");

        let headers = params.get("headers").map(|v| v);

        let body = params.get("body").and_then(|v| v.as_str());

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url_blocks_file_scheme() {
        let result = validate_url_safety("file:///etc/passwd");
        assert!(result.is_err());
        assert!(format!("{:?}", result).contains("not allowed"));
    }

    #[test]
    fn test_validate_url_blocks_private_ipv4() {
        assert!(validate_url_safety("http://127.0.0.1/secret").is_err());
        assert!(validate_url_safety("http://10.0.0.1/admin").is_err());
        assert!(validate_url_safety("http://192.168.1.1/config").is_err());
        assert!(validate_url_safety("http://172.16.0.1/internal").is_err());
        // AWS metadata endpoint
        assert!(validate_url_safety("http://169.254.169.254/latest/meta-data").is_err());
    }

    #[test]
    fn test_validate_url_blocks_localhost() {
        assert!(validate_url_safety("http://localhost/secret").is_err());
        assert!(validate_url_safety("http://localhost:3000/api").is_err());
    }

    #[test]
    fn test_validate_url_blocks_internal_hostnames() {
        assert!(validate_url_safety("http://metadata.google.internal/").is_err());
        assert!(validate_url_safety("http://something.internal/").is_err());
    }

    #[test]
    fn test_validate_url_allows_public_https() {
        assert!(validate_url_safety("https://example.com/page").is_ok());
        assert!(validate_url_safety("http://api.github.com/repos").is_ok());
    }

    #[test]
    fn test_validate_url_rejects_invalid_url() {
        assert!(validate_url_safety("not-a-url").is_err());
    }

    #[test]
    fn test_is_private_ip_loopback() {
        assert!(is_private_ip(&"127.0.0.1".parse().unwrap()));
        assert!(is_private_ip(&"::1".parse().unwrap()));
    }

    #[test]
    fn test_is_private_ip_public() {
        assert!(!is_private_ip(&"8.8.8.8".parse().unwrap()));
        assert!(!is_private_ip(&"1.1.1.1".parse().unwrap()));
    }
}
