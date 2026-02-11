//! Tool Adapters for AI Agent
//!
//! Responsibilities:
//! - MCP Tool Adapter: Convert MCP tools to Agent-callable tools
//! - Skill Tool Adapter: Convert Skills to Agent-callable tools
//!
//! Design: Use adapter pattern to bridge external tool systems to Agent

pub mod mcp_adapter;
pub mod skill_adapter;
