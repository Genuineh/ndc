//! MCP Transport Layer - JSON-RPC message handling

use serde::{Deserialize, Serialize};

/// JSON-RPC message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcMessage {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub method: Option<String>,
    pub params: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// Transport configuration
#[derive(Debug)]
pub enum TransportConfig {
    Stdio {
        command: Vec<String>,
    },
    Http {
        url: String,
    },
    Tcp {
        host: String,
        port: u16,
    },
}

/// Create transport from config
pub fn create_transport(config: &TransportConfig) -> Result<(), String> {
    match config {
        TransportConfig::Stdio { command } => {
            if command.is_empty() {
                return Err("Empty command".to_string());
            }
            Ok(())
        }
        TransportConfig::Http { url: _ } => Ok(()),
        TransportConfig::Tcp { host: _, port: _ } => Ok(()),
    }
}
