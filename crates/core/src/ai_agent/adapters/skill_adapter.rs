//! Skill Tool Adapter
//!
//! Converts Skills to Agent-callable tools
//!
//! Design:
//! - Wrap Skill definitions in Agent-compatible format
//! - Skills become specialized agent prompts
//! - Support parameter substitution for skills

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Skill definition from skill registry
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkillDef {
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub parameters: Vec<SkillParameter>,
    #[serde(default)]
    pub examples: Vec<SkillExample>,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkillParameter {
    pub name: String,
    pub r#type: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SkillExample {
    pub description: String,
    pub invocation: String,
}

/// Skill Adapter configuration
#[derive(Debug, Clone)]
pub struct SkillAdapterConfig {
    /// Whether to include skill content as system prompt
    pub include_content: bool,

    /// Parameter prefix for skill parameters
    pub param_prefix: String,
}

impl Default for SkillAdapterConfig {
    fn default() -> Self {
        Self {
            include_content: true,
            param_prefix: "skill_".to_string(),
        }
    }
}

/// Skill Tool - wraps skill for use in Agent
#[derive(Debug, Clone)]
pub struct SkillAgentTool {
    /// Original skill name
    pub skill_name: String,

    /// Agent-compatible tool name
    pub agent_name: String,

    /// Description for the agent
    pub description: String,

    /// JSON Schema for parameters
    pub parameters: Value,

    /// Skill content (for context)
    pub content: String,

    /// Configuration
    pub config: SkillAdapterConfig,
}

impl SkillAgentTool {
    /// Create new Skill Agent tool
    pub fn new(
        skill_def: SkillDef,
        config: SkillAdapterConfig,
    ) -> Self {
        let agent_name = format!("skill_{}", skill_def.name);

        // Convert skill parameters to JSON Schema
        let parameters = Self::params_to_schema(&skill_def.parameters);

        Self {
            skill_name: skill_def.name,
            agent_name,
            description: skill_def.description,
            parameters,
            content: skill_def.content,
            config,
        }
    }

    /// Convert skill parameters to JSON Schema
    fn params_to_schema(params: &[SkillParameter]) -> Value {
        let mut properties: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        let mut required = Vec::new();

        for param in params {
            let param_type = match param.r#type.to_lowercase().as_str() {
                "string" => "string",
                "number" | "int" | "integer" => "number",
                "boolean" | "bool" => "boolean",
                "array" => "array",
                "object" => "object",
                _ => "string",
            };

            let mut prop = serde_json::Map::new();
            prop.insert("type".to_string(), json!(param_type));
            prop.insert("description".to_string(), json!(param.description));

            properties.insert(param.name.clone(), json!(prop));

            if param.required {
                required.push(param.name.clone());
            }
        }

        json!({
            "type": "object",
            "properties": properties,
            "required": required,
        })
    }

    /// Get the tool schema for LLM
    pub fn to_schema(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.agent_name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

/// Skill Tool Registry - manages converted Skill tools
#[derive(Debug, Clone)]
pub struct SkillToolRegistry {
    /// All converted skill tools
    tools: HashMap<String, SkillAgentTool>,

    /// Configuration
    config: SkillAdapterConfig,
}

impl SkillToolRegistry {
    /// Create new registry
    pub fn new(config: SkillAdapterConfig) -> Self {
        Self {
            tools: HashMap::new(),
            config,
        }
    }

    /// Register a skill
    pub fn register_skill(&mut self, skill_def: SkillDef) {
        let tool = SkillAgentTool::new(skill_def, self.config.clone());
        self.tools.insert(tool.agent_name.clone(), tool);
    }

    /// Register multiple skills at once
    pub fn register_skills(&mut self, skills: Vec<SkillDef>) {
        for skill in skills {
            self.register_skill(skill);
        }
    }

    /// Get all tool names
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Get a specific tool
    pub fn get(&self, name: &str) -> Option<&SkillAgentTool> {
        self.tools.get(name)
    }

    /// Get all tools as schema
    pub fn to_schema(&self) -> Value {
        let tools: Vec<Value> = self
            .tools
            .values()
            .map(|t| t.to_schema())
            .collect();

        json!(tools)
    }

    /// Get all skill contents (for context injection)
    pub fn get_all_content(&self) -> Vec<(String, String)> {
        self.tools
            .values()
            .map(|t| (t.agent_name.clone(), t.content.clone()))
            .collect()
    }
}

impl Default for SkillToolRegistry {
    fn default() -> Self {
        Self::new(SkillAdapterConfig::default())
    }
}
