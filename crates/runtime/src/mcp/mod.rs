//! MCP (Model Context Protocol) Integration
//!
//! Responsibilities:
//! - Connect to external MCP servers
//! - Tool synchronization
//! - Prompt and resource management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::process::Command;
use tracing::{info, warn};

/// MCP Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub server_type: McpServerType,
    pub command: Option<Vec<String>>,
    pub url: Option<String>,
    pub enabled: bool,
    pub timeout_ms: u64,
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

/// MCP Manager - manages multiple MCP connections
#[derive(Debug)]
pub struct McpManager {
    /// Server configurations
    servers: HashMap<String, McpServerConfig>,
    /// Discovered tools from all servers
    tools: HashMap<String, McpTool>,
}

impl McpManager {
    /// Create a new manager
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            tools: HashMap::new(),
        }
    }

    /// Add a server configuration
    pub fn add_server(&mut self, config: McpServerConfig) {
        self.servers.insert(config.name.clone(), config);
    }

    /// Connect to all enabled servers
    pub async fn connect_all(&mut self) -> Result<(), String> {
        for (name, config) in &self.servers {
            if !config.enabled {
                warn!("MCP server {} is disabled, skipping", name);
                continue;
            }

            info!("Connecting to MCP server: {}", name);

            match config.server_type {
                McpServerType::Local => {
                    if let Some(ref cmd) = config.command {
                        Self::launch_local_server(name, cmd).await?;
                    }
                }
                McpServerType::Remote => {
                    if config.url.is_some() {
                        // Would connect to remote server
                        warn!("Remote MCP server {} not yet implemented", name);
                    }
                }
                McpServerType::Sse => {
                    warn!("SSE MCP server {} not yet implemented", name);
                }
            }
        }

        Ok(())
    }

    /// Launch a local MCP server
    async fn launch_local_server(name: &str, command: &[String]) -> Result<(), String> {
        if command.is_empty() {
            return Err("Empty command".to_string());
        }

        let mut cmd = Command::new(&command[0]);
        if command.len() > 1 {
            cmd.args(&command[1..]);
        }

        let _child = cmd.spawn()
            .map_err(|e| format!("Failed to spawn MCP server {}: {}", name, e))?;

        Ok(())
    }

    /// Get all discovered tools
    pub fn get_tools(&self) -> &HashMap<String, McpTool> {
        &self.tools
    }

    /// Add a tool from an MCP server
    pub fn add_tool(&mut self, server_name: &str, tool: McpTool) {
        let key = format!("{}_{}", server_name, tool.name);
        self.tools.insert(key, tool);
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}
