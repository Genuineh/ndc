//! MCP (Model Context Protocol) Integration
//!
//! Responsibilities:
//! - Connect to external MCP servers
//! - Tool synchronization
//! - Prompt and resource management
//! - OAuth authentication

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// MCP Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub server_type: McpServerType,
    pub command: Option<Vec<String>>,
    pub url: Option<String>,
    pub enabled: bool,
    pub timeout_ms: u64,
    pub oauth: Option<McpOAuthConfig>,
    pub headers: Option<HashMap<String, String>>,
}

/// OAuth configuration for remote servers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpOAuthConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scope: String,
    pub token_url: String,
    pub authorization_url: Option<String>,
    pub redirect_uri: Option<String>,
}

/// MCP Server type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpServerType {
    Local,
    Remote,
    Sse,
}

/// MCP Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: serde_json::Value,
}

/// MCP Prompt definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    pub name: String,
    pub description: String,
    pub arguments: Vec<McpPromptArgument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub r#type: Option<String>,
}

/// MCP Resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub mime_type: String,
}

/// MCP result wrapper
#[derive(Debug, Clone)]
pub struct McpResult {
    pub content: String,
    pub is_error: bool,
    pub tool_name: String,
}

/// Active MCP connection
struct McpConnection {
    config: McpServerConfig,
    child: Option<tokio::process::Child>,
    transport: Option<Box<dyn McpTransport>>,
}

impl std::fmt::Debug for McpConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpConnection")
            .field("config", &self.config.name)
            .field("child", &self.child.is_some())
            .finish_non_exhaustive()
    }
}

/// Transport trait for MCP communication
#[async_trait::async_trait]
pub trait McpTransport: Send {
    async fn send(&mut self, message: &serde_json::Value) -> Result<serde_json::Value, String>;
    async fn close(&mut self);
}

/// Stdio transport for local MCP servers
#[derive(Debug)]
pub struct StdioTransport {
    stdin: Option<tokio::process::ChildStdin>,
    stdout: Option<tokio::io::BufReader<tokio::process::ChildStdout>>,
}

#[async_trait::async_trait]
impl McpTransport for StdioTransport {
    async fn send(&mut self, message: &serde_json::Value) -> Result<serde_json::Value, String> {
        let json = serde_json::to_string(message)
            .map_err(|e| format!("Serialize failed: {}", e))?;

        let payload = format!("Content-Length: {}\n\n{}", json.len(), json);

        if let Some(ref mut stdin) = self.stdin {
            tokio::io::AsyncWriteExt::write_all(stdin, payload.as_bytes())
                .await
                .map_err(|e| format!("Write failed: {}", e))?;
        }

        Ok(serde_json::Value::Null)
    }

    async fn close(&mut self) {
        if let Some(mut stdin) = self.stdin.take() {
            tokio::io::AsyncWriteExt::shutdown(&mut stdin).await.ok();
        }
        self.stdout = None;
    }
}

/// HTTP transport for remote MCP servers
#[derive(Debug)]
pub struct HttpTransport {
    url: String,
    client: reqwest::Client,
    token: Option<String>,
}

impl HttpTransport {
    pub fn new(url: String, token: Option<String>) -> Self {
        let client = reqwest::Client::new();
        Self { url, client, token }
    }
}

#[async_trait::async_trait]
impl McpTransport for HttpTransport {
    async fn send(&mut self, message: &serde_json::Value) -> Result<serde_json::Value, String> {
        let mut request = self.client.post(&self.url).json(message);

        if let Some(ref token) = self.token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        response.json().await
            .map_err(|e| format!("Parse failed: {}", e))
    }

    async fn close(&mut self) {
        self.token = None;
    }
}

/// MCP Manager - manages multiple MCP connections
#[derive(Debug)]
pub struct McpManager {
    /// Server configurations
    servers: HashMap<String, McpServerConfig>,
    /// Active connections
    connections: HashMap<String, McpConnection>,
    /// Discovered tools from all servers
    tools: HashMap<String, McpTool>,
    /// Discovered prompts from all servers
    prompts: HashMap<String, McpPrompt>,
    /// Discovered resources from all servers
    resources: HashMap<String, McpResource>,
    /// OAuth tokens cache
    oauth_tokens: HashMap<String, String>,
}

impl McpManager {
    /// Create a new manager
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            connections: HashMap::new(),
            tools: HashMap::new(),
            prompts: HashMap::new(),
            resources: HashMap::new(),
            oauth_tokens: HashMap::new(),
        }
    }

    /// Add a server configuration
    pub fn add_server(&mut self, config: McpServerConfig) {
        self.servers.insert(config.name.clone(), config);
    }

    /// Load servers from YAML config
    pub fn load_config(&mut self, config_path: &PathBuf) -> Result<(), String> {
        let content = std::fs::read_to_string(config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;

        let configs: Vec<McpServerConfig> = serde_yaml::from_str(&content)
            .map_err(|e| format!("Failed to parse config: {}", e))?;

        for config in configs {
            self.add_server(config);
        }

        Ok(())
    }

    /// Connect to all enabled servers
    pub async fn connect_all(&mut self) -> Result<(), String> {
        // Collect server names first to avoid borrowing issues
        let server_names: Vec<String> = self.servers.keys().cloned().collect();

        for name in server_names {
            let config = self.servers.get(&name).expect("Server should exist");
            if !config.enabled {
                warn!("MCP server {} is disabled, skipping", name);
                continue;
            }

            info!("Connecting to MCP server: {}", name);

            if let Err(e) = self.connect_server(&name).await {
                warn!("Failed to connect to MCP server {}: {}", name, e);
            }
        }

        Ok(())
    }

    /// Connect to a specific server
    pub async fn connect_server(&mut self, name: &str) -> Result<(), String> {
        let config = self.servers.get(name)
            .ok_or_else(|| format!("Unknown server: {}", name))?
            .clone();

        // Handle OAuth if needed
        if let Some(ref oauth) = config.oauth {
            if self.oauth_tokens.get(name).is_none() {
                // Get new token
                let token = self.obtain_oauth_token(name, oauth).await?;
                self.oauth_tokens.insert(name.to_string(), token.clone());
            }
        }

        // Create transport based on server type
        let transport: Option<Box<dyn McpTransport>> = match config.server_type {
            McpServerType::Local => {
                if let Some(ref cmd) = config.command {
                    Some(Box::new(self.create_stdio_transport(name, cmd).await?))
                } else {
                    None
                }
            }
            McpServerType::Remote => {
                if let Some(ref url) = config.url {
                    let token = self.oauth_tokens.get(name).cloned();
                    Some(Box::new(HttpTransport::new(url.clone(), token)))
                } else {
                    None
                }
            }
            McpServerType::Sse => {
                warn!("SSE transport not yet implemented");
                None
            }
        };

        // Initialize connection
        let connection = McpConnection {
            config: config.clone(),
            child: None,
            transport,
        };

        self.connections.insert(name.to_string(), connection);

        // Discover tools, prompts, resources
        self.discover_resources(name).await?;

        Ok(())
    }

    /// Create stdio transport for local server
    async fn create_stdio_transport(&mut self, name: &str, command: &[String]) -> Result<StdioTransport, String> {
        if command.is_empty() {
            return Err("Empty command".to_string());
        }

        let mut cmd = Command::new(&command[0]);
        if command.len() > 1 {
            cmd.args(&command[1..]);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());

        let mut child = cmd.spawn()
            .map_err(|e| format!("Failed to spawn MCP server {}: {}", name, e))?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take().map(tokio::io::BufReader::new);

        Ok(StdioTransport { stdin, stdout })
    }

    /// Obtain OAuth token
    async fn obtain_oauth_token(&self, _name: &str, _oauth: &McpOAuthConfig) -> Result<String, String> {
        // In a real implementation, this would:
        // 1. Check for cached token
        // 2. If expired, refresh using client_id + client_secret
        // 3. If no token, initiate authorization flow

        // For now, check environment variable
        let token = std::env::var(format!("NDC_MCP_TOKEN_{}", _name.to_uppercase()))
            .or_else(|_| std::env::var("NDC_MCP_TOKEN"))
            .map_err(|_| "No OAuth token available".to_string())?;

        Ok(token)
    }

    /// Discover resources from a server
    async fn discover_resources(&mut self, server_name: &str) -> Result<(), String> {
        // Send JSON-RPC messages to discover resources
        // In a full implementation, this would send:
        // - tools/list
        // - prompts/list
        // - resources/list

        debug!("Discovering resources from MCP server: {}", server_name);

        Ok(())
    }

    /// Disconnect from a server
    pub async fn disconnect(&mut self, name: &str) -> Result<(), String> {
        if let Some(mut connection) = self.connections.remove(name) {
            if let Some(ref mut transport) = connection.transport {
                transport.close().await;
            }
            if let Some(ref mut child) = connection.child {
                child.kill().await.ok();
            }
        }

        // Remove discovered resources
        self.tools.retain(|k, _| !k.starts_with(&format!("{}_", name)));
        self.prompts.retain(|k, _| !k.starts_with(&format!("{}_", name)));
        self.resources.retain(|k, _| !k.starts_with(&format!("{}_", name)));

        Ok(())
    }

    /// Disconnect from all servers
    pub async fn disconnect_all(&mut self) {
        let names: Vec<String> = self.connections.keys().cloned().collect();
        for name in names {
            self.disconnect(&name).await.ok();
        }
    }

    /// Get all discovered tools
    pub fn get_tools(&self) -> &HashMap<String, McpTool> {
        &self.tools
    }

    /// Get all discovered prompts
    pub fn get_prompts(&self) -> &HashMap<String, McpPrompt> {
        &self.prompts
    }

    /// Get all discovered resources
    pub fn get_resources(&self) -> &HashMap<String, McpResource> {
        &self.resources
    }

    /// Call a tool on a specific server
    pub async fn call_tool(&mut self, server_name: &str, tool_name: &str, args: serde_json::Value) -> Result<McpResult, String> {
        let connection = self.connections.get_mut(server_name)
            .ok_or_else(|| format!("Not connected to server: {}", server_name))?;

        let transport = connection.transport.as_mut()
            .ok_or_else(|| format!("No transport for server: {}", server_name))?;

        // Build JSON-RPC request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": args
            },
            "id": 1
        });

        let response = transport.send(&request).await?;

        // Parse response
        let content = response.to_string();

        Ok(McpResult {
            content,
            is_error: false,
            tool_name: tool_name.to_string(),
        })
    }

    /// Get a tool by name (including server prefix)
    pub fn get_tool(&self, name: &str) -> Option<&McpTool> {
        self.tools.get(name)
    }

    /// Search tools by name or description
    pub fn search_tools(&self, query: &str) -> Vec<&McpTool> {
        let query = query.to_lowercase();
        self.tools.values()
            .filter(|tool| {
                tool.name.to_lowercase().contains(&query) ||
                tool.description.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Get connection status
    pub fn is_connected(&self, name: &str) -> bool {
        self.connections.contains_key(name)
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_serde() {
        let config = McpServerConfig {
            name: "filesystem".to_string(),
            server_type: McpServerType::Local,
            command: Some(vec!["npx".to_string(), "@modelcontextplugin/server-filesystem".to_string()]),
            url: None,
            enabled: true,
            timeout_ms: 30000,
            oauth: None,
            headers: None,
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: McpServerConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(parsed.name, "filesystem");
        assert!(parsed.enabled);
    }

    #[test]
    fn test_tool_serde() {
        let tool = McpTool {
            name: "read_file".to_string(),
            description: "Read a file from the filesystem".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        };

        let json = serde_json::to_string(&tool).unwrap();
        let parsed: McpTool = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "read_file");
        assert!(parsed.input_schema.is_object());
    }

    #[test]
    fn test_mcp_manager_new() {
        let manager = McpManager::new();
        assert!(manager.get_tools().is_empty());
        assert!(manager.get_prompts().is_empty());
        assert!(manager.get_resources().is_empty());
    }

    #[test]
    fn test_add_server() {
        let mut manager = McpManager::new();

        let config = McpServerConfig {
            name: "test-server".to_string(),
            server_type: McpServerType::Local,
            command: Some(vec!["echo".to_string()]),
            url: None,
            enabled: true,
            timeout_ms: 30000,
            oauth: None,
            headers: None,
        };

        manager.add_server(config);

        assert!(manager.servers.contains_key("test-server"));
    }

    #[test]
    fn test_search_tools() {
        let mut manager = McpManager::new();

        manager.tools.insert("fs_read".to_string(), McpTool {
            name: "read".to_string(),
            description: "Read files".to_string(),
            input_schema: serde_json::json!({}),
        });

        manager.tools.insert("fs_write".to_string(), McpTool {
            name: "write".to_string(),
            description: "Write files".to_string(),
            input_schema: serde_json::json!({}),
        });

        let results = manager.search_tools("read");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "read");
    }
}
