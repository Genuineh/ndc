//! Decision engine core traits and implementation

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

use ndc_core::{
    Intent, Verdict, AgentRole, TaskId,
};

/// Policy state tracking
#[derive(Debug, Clone)]
pub struct PolicyState {
    pub active_rules: Vec<String>,
    pub human_interventions: u32,
    pub denied_intents: u32,
}

impl PolicyState {
    pub fn new() -> Self {
        Self {
            active_rules: Vec::new(),
            human_interventions: 0,
            denied_intents: 0,
        }
    }
}

/// Validation context for intent evaluation
#[derive(Debug, Clone)]
pub struct ValidationContext {
    pub task_id: TaskId,
    pub agent_role: AgentRole,
}

impl ValidationContext {
    pub fn new(intent: &Intent, _policy: &Arc<RwLock<PolicyState>>) -> Self {
        Self {
            task_id: intent.task_id,
            agent_role: intent.agent_role,
        }
    }
}

/// Validation result
#[derive(Debug, Clone)]
pub enum ValidationResult {
    Pass,
    Fail { reason: String },
    WarningPass { warnings: Vec<String> },
}

/// Decision engine trait
#[async_trait]
pub trait DecisionEngine: Send + Sync {
    /// Evaluate an intent and return a verdict
    async fn evaluate(&self, intent: Intent) -> Verdict;

    /// Register a validator
    fn register_validator(&mut self, validator: Box<dyn Validator>);

    /// Get current policy state
    fn policy_state(&self) -> PolicyState;
}

/// Validator trait for composable policy units
#[async_trait]
pub trait Validator: Send + Sync {
    /// Validate an intent
    async fn validate(&self, intent: &Intent, context: &ValidationContext) -> ValidationResult;

    /// Validator name
    fn name(&self) -> &str;

    /// Validator priority (lower = higher priority)
    fn priority(&self) -> u32;
}

/// Policy decision engine implementation
pub struct PolicyDecisionEngine {
    validators: Vec<Box<dyn Validator>>,
    policy_state: Arc<RwLock<PolicyState>>,
}

impl PolicyDecisionEngine {
    pub fn new() -> Self {
        Self {
            validators: Vec::new(),
            policy_state: Arc::new(RwLock::new(PolicyState::new())),
        }
    }

    pub fn with_state(policy_state: Arc<RwLock<PolicyState>>) -> Self {
        Self {
            validators: Vec::new(),
            policy_state,
        }
    }
}

impl Default for PolicyDecisionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DecisionEngine for PolicyDecisionEngine {
    async fn evaluate(&self, intent: Intent) -> Verdict {
        let context = ValidationContext::new(&intent, &self.policy_state);

        for validator in &self.validators {
            match validator.validate(&intent, &context).await {
                ValidationResult::Pass => continue,
                ValidationResult::WarningPass { .. } => continue,
                ValidationResult::Fail { reason } => {
                    return Verdict::Deny {
                        reason,
                        code: ndc_core::ErrorCode::ActionNotAllowed,
                    };
                }
            }
        }

        Verdict::Allow
    }

    fn register_validator(&mut self, validator: Box<dyn Validator>) {
        self.validators.push(validator);
        self.validators.sort_by_key(|v| v.priority());
    }

    fn policy_state(&self) -> PolicyState {
        self.policy_state.blocking_read().clone()
    }
}
