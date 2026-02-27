//! Skill Execution Engine
//!
//! Responsibilities:
//! - Execute skills with parameter substitution
//! - Integrate with LLM for AI-powered skills
//! - Chain multiple skills together
//! - Track execution state and results

use super::{Skill, SkillRegistry};
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Context for skill execution
#[derive(Clone)]
pub struct SkillExecutionContext {
    /// Current working directory
    pub working_dir: std::path::PathBuf,
    /// User-provided variables
    pub variables: HashMap<String, String>,
    /// LLM provider for AI skills
    pub llm_provider: Option<Arc<dyn LlmProvider>>,
    /// Tool registry for executing tool calls
    pub tool_registry: Option<Arc<dyn ToolRegistryProvider>>,
    /// Maximum execution steps
    pub max_steps: usize,
    /// Current step count
    pub step_count: usize,
    /// Execution history
    pub history: Vec<SkillExecutionStep>,
}

/// Provider trait for LLM operations
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String, String>;
}

/// Provider trait for tool registry access
#[async_trait::async_trait]
pub trait ToolRegistryProvider: Send + Sync {
    async fn list_tools(&self) -> Vec<String>;
    async fn execute_tool(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}

/// A single execution step
#[derive(Debug, Clone)]
pub struct SkillExecutionStep {
    pub step_type: ExecutionStepType,
    pub content: String,
    pub result: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub enum ExecutionStepType {
    Thought,
    Action,
    Observation,
    Final,
}

/// Skill execution result
#[derive(Debug, Clone)]
pub struct SkillResult {
    pub success: bool,
    pub output: String,
    pub steps: Vec<SkillExecutionStep>,
    pub tool_calls: Vec<ToolCall>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
    pub result: Option<String>,
}

/// Skill Executor
#[derive(Clone)]
pub struct SkillExecutor {
    registry: Arc<RwLock<SkillRegistry>>,
    context: Option<SkillExecutionContext>,
}

impl SkillExecutor {
    pub fn new(registry: Arc<RwLock<SkillRegistry>>) -> Self {
        Self {
            registry,
            context: None,
        }
    }

    pub fn with_context(mut self, context: SkillExecutionContext) -> Self {
        self.context = Some(context);
        self
    }

    /// Execute a skill by name with parameters
    pub async fn execute(
        &mut self,
        name: &str,
        parameters: HashMap<String, String>,
    ) -> Result<SkillResult, String> {
        let skill = {
            let registry = self.registry.read().await;
            match registry.get(name) {
                Some(s) => s.clone(),
                None => return Err(format!("Skill '{}' not found", name)),
            }
        };

        let start_time = std::time::Instant::now();
        let mut steps = Vec::new();

        // Build execution context if not set
        if self.context.is_none() {
            self.context = Some(SkillExecutionContext::default());
        }

        // Merge parameters into variables
        if let Some(context) = self.context.as_mut() {
            for (key, value) in parameters {
                context.variables.insert(key, value);
            }
        }

        // Execute skill content
        let output = {
            let context = self.context.as_ref().expect("context set above");
            self.execute_skill_content(&skill, context, &mut steps)
                .await?
        };

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(SkillResult {
            success: true,
            output,
            steps,
            tool_calls: Vec::new(),
            error: None,
            execution_time_ms,
        })
    }

    /// Execute skill content with template substitution
    async fn execute_skill_content(
        &self,
        skill: &Skill,
        context: &SkillExecutionContext,
        steps: &mut Vec<SkillExecutionStep>,
    ) -> Result<String, String> {
        let content = &skill.content;

        // Process content based on skill type
        if self.is_llm_skill(content) {
            let mut ctx = context.clone();
            self.execute_llm_skill(content, &mut ctx, steps).await
        } else {
            self.execute_template_skill(content, context).await
        }
    }

    /// Check if skill is LLM-powered
    fn is_llm_skill(&self, content: &str) -> bool {
        content.contains("{{llm.") || content.contains("{{thought}}") || content.contains("@")
    }

    /// Execute LLM-powered skill
    async fn execute_llm_skill(
        &self,
        content: &str,
        context: &mut SkillExecutionContext,
        steps: &mut Vec<SkillExecutionStep>,
    ) -> Result<String, String> {
        let provider = context
            .llm_provider
            .as_ref()
            .ok_or_else(|| "LLM provider required for AI skill".to_string())?;

        // Substitute variables
        let processed = self.substitute_variables(content, context)?;

        // Add thought step
        steps.push(SkillExecutionStep {
            step_type: ExecutionStepType::Thought,
            content: "Analyzing skill task".to_string(),
            result: None,
            timestamp: chrono::Utc::now(),
        });

        // Call LLM
        let result = provider
            .complete(&processed)
            .await
            .map_err(|e| format!("LLM call failed: {}", e))?;

        // Add observation step
        steps.push(SkillExecutionStep {
            step_type: ExecutionStepType::Observation,
            content: result.clone(),
            result: None,
            timestamp: chrono::Utc::now(),
        });

        Ok(result)
    }

    /// Execute template-based skill
    async fn execute_template_skill(
        &self,
        content: &str,
        context: &SkillExecutionContext,
    ) -> Result<String, String> {
        let processed = self.substitute_variables(content, context)?;

        // Parse and execute actions
        let lines: Vec<&str> = processed.lines().collect();
        let mut output = String::new();

        for line in lines {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                output.push_str(line);
                output.push('\n');
            } else if trimmed.starts_with("```") {
                output.push_str(line);
                output.push('\n');
            } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                output.push_str(line);
                output.push('\n');
            } else {
                output.push_str(line);
                output.push('\n');
            }
        }

        Ok(output)
    }

    /// Substitute template variables
    pub fn substitute_variables(
        &self,
        content: &str,
        context: &SkillExecutionContext,
    ) -> Result<String, String> {
        let mut result = content.to_string();

        // Variable substitution: {{variable}}
        if let Some(re) = Regex::new(r"\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}")
            .ok()
            .as_ref()
        {
            result = re
                .replace_all(&result, |caps: &regex::Captures| {
                    let var_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    context
                        .variables
                        .get(var_name)
                        .map(|s| s.as_str())
                        .unwrap_or("")
                })
                .to_string();
        }

        // Environment variables: {{env.VAR}}
        if let Some(re) = Regex::new(r"\{\{env\.([A-Z_][A-Z0-9_]*)\}\}").ok().as_ref() {
            result = re
                .replace_all(&result, |caps: &regex::Captures| {
                    let var_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    std::env::var(var_name).ok().unwrap_or_default()
                })
                .to_string();
        }

        // Built-in variables
        result = result.replace("{{cwd}}", context.working_dir.to_string_lossy().as_ref());
        result = result.replace("{{timestamp}}", &chrono::Utc::now().to_rfc3339());

        Ok(result)
    }

    /// Execute skill with examples
    pub async fn execute_with_example(
        &self,
        name: &str,
        invocation: &str,
    ) -> Result<SkillResult, String> {
        let skill = {
            let registry = self.registry.read().await;
            match registry.get(name) {
                Some(s) => s.clone(),
                None => return Err(format!("Skill '{}' not found", name)),
            }
        };

        // Extract parameters from invocation
        let parameters = Self::parse_invocation(invocation, &skill);

        let mut executor = self.clone();
        executor.context = self.context.clone();
        executor.execute(name, parameters).await
    }

    /// Parse skill invocation string into parameters
    fn parse_invocation(invocation: &str, skill: &Skill) -> HashMap<String, String> {
        let mut params = HashMap::new();

        // Simple key=value parsing
        if let Some(re) = Regex::new(r"--([a-zA-Z_][a-zA-Z0-9_]*)\s+(\S+)")
            .ok()
            .as_ref()
            && let Some(cap) = re.captures(invocation)
            && let (Some(key_match), Some(value_match)) = (cap.get(1), cap.get(2))
        {
            params.insert(
                key_match.as_str().to_string(),
                value_match.as_str().to_string(),
            );
        }

        // Check required parameters
        for param in &skill.parameters {
            if param.required && !params.contains_key(&param.name) {
                params.insert(param.name.clone(), format!("{{{{ {} }}}}", param.name));
            }
        }

        params
    }

    /// Execute multiple skills in sequence
    pub async fn execute_chain(
        &mut self,
        chain: &[(&str, HashMap<String, String>)],
    ) -> Result<Vec<SkillResult>, String> {
        let mut results = Vec::new();

        for (name, params) in chain {
            let result = self.execute(name, params.clone()).await?;
            results.push(result.clone());

            if !result.success {
                return Err(format!("Skill chain failed at '{}'", name));
            }
        }

        Ok(results)
    }
}

impl Default for SkillExecutionContext {
    fn default() -> Self {
        Self {
            working_dir: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            variables: HashMap::new(),
            llm_provider: None,
            tool_registry: None,
            max_steps: 100,
            step_count: 0,
            history: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::SkillParameter;
    use super::*;

    fn create_test_skill(name: &str, content: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: format!("Test skill: {}", name),
            category: Some("test".to_string()),
            tags: vec!["test".to_string()],
            parameters: vec![],
            examples: vec![],
            content: content.to_string(),
        }
    }

    #[tokio::test]
    async fn test_skill_executor_new() {
        let registry = Arc::new(RwLock::new(SkillRegistry::new()));
        let executor = SkillExecutor::new(registry);

        assert!(executor.context.is_none());
    }

    #[tokio::test]
    async fn test_skill_executor_with_context() {
        let registry = Arc::new(RwLock::new(SkillRegistry::new()));
        let context = SkillExecutionContext {
            working_dir: std::path::PathBuf::from("/tmp"),
            variables: HashMap::new(),
            llm_provider: None,
            tool_registry: None,
            max_steps: 50,
            step_count: 0,
            history: Vec::new(),
        };

        let executor = SkillExecutor::new(registry).with_context(context);

        assert!(executor.context.is_some());
        assert_eq!(
            executor.context.as_ref().unwrap().working_dir,
            std::path::PathBuf::from("/tmp")
        );
    }

    #[tokio::test]
    async fn test_execute_skill_not_found() {
        let registry = Arc::new(RwLock::new(SkillRegistry::new()));
        let mut executor = SkillExecutor::new(registry);

        let result = executor.execute("nonexistent", HashMap::new()).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_skill_execution() {
        let registry = Arc::new(RwLock::new(SkillRegistry::new()));
        registry
            .write()
            .await
            .add_discovery_path(std::path::PathBuf::from("."));

        let skill = create_test_skill(
            "test-skill",
            r#"# Test Skill

This is a test skill content.

## Usage
- First step
- Second step
"#,
        );

        registry
            .write()
            .await
            .skills
            .insert(skill.name.clone(), skill);

        let mut executor = SkillExecutor::new(Arc::clone(&registry));
        let result = executor.execute("test-skill", HashMap::new()).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Test Skill"));
    }

    #[tokio::test]
    async fn test_variable_substitution() {
        let registry = Arc::new(RwLock::new(SkillRegistry::new()));
        let executor = SkillExecutor::new(registry);

        let content = "Hello {{name}}, you are from {{city}}!";
        let mut context = SkillExecutionContext::default();
        context
            .variables
            .insert("name".to_string(), "Alice".to_string());
        context
            .variables
            .insert("city".to_string(), "Beijing".to_string());

        let result = executor.substitute_variables(content, &context);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello Alice, you are from Beijing!");
    }

    #[tokio::test]
    async fn test_missing_variables() {
        let registry = Arc::new(RwLock::new(SkillRegistry::new()));
        let executor = SkillExecutor::new(registry);

        let content = "Hello {{name}}, {{missing}}!";
        let context = SkillExecutionContext::default();

        let result = executor.substitute_variables(content, &context);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello , !");
    }

    #[tokio::test]
    async fn test_env_variable_substitution() {
        let registry = Arc::new(RwLock::new(SkillRegistry::new()));
        let executor = SkillExecutor::new(registry);

        let content = "HOME={{env.HOME}}";
        let context = SkillExecutionContext::default();

        let result = executor.substitute_variables(content, &context);

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.starts_with("HOME=/"));
    }

    #[tokio::test]
    async fn test_builtin_variables() {
        let registry = Arc::new(RwLock::new(SkillRegistry::new()));
        let executor = SkillExecutor::new(registry);

        let content = "CWD={{cwd}}";
        let context = SkillExecutionContext::default();

        let result = executor.substitute_variables(content, &context);

        assert!(result.is_ok());
        let output = result.unwrap();
        // The cwd in test is the current directory
        assert!(output.contains("CWD="));
    }

    #[tokio::test]
    async fn test_parse_invocation() {
        let skill = Skill {
            name: "test".to_string(),
            description: "test".to_string(),
            category: None,
            tags: vec![],
            parameters: vec![SkillParameter {
                name: "path".to_string(),
                r#type: "string".to_string(),
                description: "Path to file".to_string(),
                required: true,
            }],
            examples: vec![],
            content: "test".to_string(),
        };

        let invocation = "@test --path /tmp/test";
        let params = SkillExecutor::parse_invocation(invocation, &skill);

        assert!(params.contains_key("path"));
    }

    #[tokio::test]
    async fn test_llm_skill_requires_provider() {
        let registry = Arc::new(RwLock::new(SkillRegistry::new()));
        registry
            .write()
            .await
            .add_discovery_path(std::path::PathBuf::from("."));

        let skill = create_test_skill(
            "llm-skill",
            r#"# LLM Skill

{{llm.thought}}
"#,
        );

        registry
            .write()
            .await
            .skills
            .insert(skill.name.clone(), skill);

        let mut executor = SkillExecutor::new(Arc::clone(&registry));
        let result = executor.execute("llm-skill", HashMap::new()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execution_steps() {
        let registry = Arc::new(RwLock::new(SkillRegistry::new()));
        registry
            .write()
            .await
            .add_discovery_path(std::path::PathBuf::from("."));

        let skill = create_test_skill(
            "step-skill",
            r#"# Step Skill

Just content.
"#,
        );

        registry
            .write()
            .await
            .skills
            .insert(skill.name.clone(), skill);

        let mut executor = SkillExecutor::new(Arc::clone(&registry));
        let result = executor.execute("step-skill", HashMap::new()).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.success);
        assert!(result.execution_time_ms < 60_000);
    }

    #[tokio::test]
    async fn test_execute_with_example() {
        let registry = Arc::new(RwLock::new(SkillRegistry::new()));
        registry
            .write()
            .await
            .add_discovery_path(std::path::PathBuf::from("."));

        let skill = create_test_skill(
            "example-skill",
            r#"# Example Skill

Path: {{path}}
"#,
        );

        registry
            .write()
            .await
            .skills
            .insert(skill.name.clone(), skill);

        let executor = SkillExecutor::new(Arc::clone(&registry));
        let result = executor
            .execute_with_example("example-skill", r#"@example-skill --path /test/path"#)
            .await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.output.contains("/test/path"));
    }
}
