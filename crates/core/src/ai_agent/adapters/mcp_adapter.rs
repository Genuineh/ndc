//! MCP Tool Adapter
//!
//! Converts MCP tools to Agent-callable tools
//!
//! Design:
//! - Wrap MCP tool definitions in Agent-compatible format
//! - Handle MCP tool invocation with proper error handling
//! - Support both sync and async MCP transports

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

/// MCP Tool definition from MCP protocol
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpToolDef {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: Value,
}

/// MCP Tool result from MCP protocol
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
}

/// MCP Adapter configuration
#[derive(Debug, Clone)]
pub struct McpAdapterConfig {
    /// Server name for identification
    pub server_name: String,

    /// Tool name prefix (e.g., "mcp_filesystem_read" instead of just "read")
    pub use_prefix: bool,

    /// Default timeout for tool calls (ms)
    pub timeout_ms: u64,
}

impl Default for McpAdapterConfig {
    fn default() -> Self {
        Self {
            server_name: "unknown".to_string(),
            use_prefix: true,
            timeout_ms: 30000,
        }
    }
}

/// MCP Tool - wraps MCP tool for use in Agent
#[derive(Debug, Clone)]
pub struct McpAgentTool {
    /// Original MCP tool name
    pub mcp_name: String,

    /// Agent-compatible tool name
    pub agent_name: String,

    /// Description for the agent
    pub description: String,

    /// JSON Schema for parameters
    pub parameters: Value,

    /// Server configuration
    pub config: McpAdapterConfig,
}

impl McpAgentTool {
    /// Create new MCP Agent tool
    pub fn new(
        mcp_name: String,
        description: String,
        input_schema: Value,
        config: McpAdapterConfig,
    ) -> Self {
        let agent_name = if config.use_prefix {
            format!("{}_{}", config.server_name, mcp_name)
        } else {
            mcp_name.clone()
        };

        Self {
            mcp_name,
            agent_name,
            description,
            parameters: input_schema,
            config,
        }
    }

    /// Get the tool schema for LLM
    pub fn to_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": self.description,
                },
                "description": self.description,
                "parameters": self.parameters,
            },
            "required": ["name"]
        })
    }
}

/// MCP Adapter trait - for different MCP transport implementations
#[async_trait]
pub trait McpTransportAdapter: Send + Sync {
    /// Call an MCP tool
    async fn call_tool(&self, tool_name: &str, arguments: &str) -> Result<String, String>;
}

/// MCP Tool Registry - manages converted MCP tools
#[derive(Clone)]
pub struct McpToolRegistry {
    /// All converted MCP tools
    tools: HashMap<String, McpAgentTool>,

    /// Transport adapter for invoking tools
    transport: Option<Arc<dyn McpTransportAdapter>>,

    /// Server configuration
    config: McpAdapterConfig,
}

impl McpToolRegistry {
    /// Create new registry
    pub fn new(config: McpAdapterConfig) -> Self {
        Self {
            tools: HashMap::new(),
            transport: None,
            config,
        }
    }

    /// Register an MCP tool
    pub fn register_tool(&mut self, tool_def: McpToolDef) {
        let tool = McpAgentTool::new(
            tool_def.name.clone(),
            tool_def.description,
            tool_def.input_schema,
            self.config.clone(),
        );
        self.tools.insert(tool.agent_name.clone(), tool);
    }

    /// Register multiple tools at once
    pub fn register_tools(&mut self, tools: Vec<McpToolDef>) {
        for tool in tools {
            self.register_tool(tool);
        }
    }

    /// Set the transport adapter
    pub fn set_transport(&mut self, transport: Arc<dyn McpTransportAdapter>) {
        self.transport = Some(transport);
    }

    /// Get all tool names
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Get a specific tool
    pub fn get(&self, name: &str) -> Option<&McpAgentTool> {
        self.tools.get(name)
    }

    /// Get all tools as schema
    pub fn to_schema(&self) -> Value {
        let tools: Vec<Value> = self
            .tools
            .values()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.agent_name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();

        json!(tools)
    }

    /// Invoke a tool
    pub async fn invoke(&self, name: &str, arguments: &str) -> Result<String, String> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| "MCP transport not configured".to_string())?;

        transport.call_tool(name, arguments).await
    }
}
