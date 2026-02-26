//! Model Selector - Adaptive LLM model selection based on task characteristics
//!
//! Selects optimal LLM model based on:
//! - Task risk level
//! - Invariant density
//! - Complexity
//! - Cross-module impact
//!
//! Principles:
//! - Low risk + high invariant density -> Fast/Cheap model
//! - Medium risk -> Balanced model
//! - High risk / cross-module -> Most capable model

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// LLM Provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LlmProvider {
    /// Fast, cheap, good for simple tasks
    Fast,
    /// Balanced capability and speed
    Balanced,
    /// Most capable, for complex tasks
    Powerful,
    /// Maximum reasoning capability
    MaxReasoning,
}

/// Task entropy - measure of uncertainty/complexity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskEntropy {
    /// Simple, well-defined task
    Low,
    /// Moderate uncertainty
    Medium,
    /// High uncertainty, needs more reasoning
    High,
    /// Very high uncertainty, complex reasoning needed
    Extreme,
}

/// Risk level for a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskRiskLevel {
    /// Safe to proceed
    Low,
    /// Caution advised
    Medium,
    /// Careful planning needed
    High,
    /// Requires human oversight
    Critical,
}

/// Task characteristics for model selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCharacteristics {
    /// Task entropy level
    pub entropy: TaskEntropy,

    /// Risk level
    pub risk_level: TaskRiskLevel,

    /// Number of files involved
    pub file_count: usize,

    /// Number of modules affected
    pub module_count: usize,

    /// Number of API calls involved
    pub api_call_count: usize,

    /// Density of applicable invariants (0.0 - 1.0)
    pub invariant_density: f32,

    /// Does task span multiple modules?
    pub is_cross_module: bool,

    /// Does task involve external APIs?
    pub has_external_apis: bool,

    /// Estimated subtask count
    pub subtask_count: usize,
}

/// Selection factors with weights
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionWeights {
    /// Weight for entropy factor
    pub entropy_weight: f32,
    /// Weight for risk factor
    pub risk_weight: f32,
    /// Weight for invariant density (negative - more invariants = simpler)
    pub invariant_density_weight: f32,
    /// Weight for cross-module factor
    pub cross_module_weight: f32,
    /// Weight for complexity (file/module count)
    pub complexity_weight: f32,
}

impl Default for SelectionWeights {
    fn default() -> Self {
        Self {
            entropy_weight: 0.25,
            risk_weight: 0.30,
            invariant_density_weight: -0.15,
            cross_module_weight: 0.15,
            complexity_weight: 0.15,
        }
    }
}

/// Model selection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelection {
    /// Selected provider
    pub provider: LlmProvider,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,

    /// Reasoning for selection
    pub reasoning: Vec<String>,

    /// Alternative providers (if any)
    pub alternatives: Vec<(LlmProvider, f32)>,

    /// Model-specific recommendations
    pub recommendations: Vec<String>,
}

/// Model Selector - Selects optimal LLM for a task
#[derive(Debug, Clone)]
pub struct ModelSelector {
    /// Selection weights
    weights: SelectionWeights,

    /// Provider capabilities
    capabilities: HashMap<LlmProvider, ProviderCapabilities>,
}

/// Capabilities of each provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Max tokens for context
    pub max_context_tokens: u32,

    /// Strengths
    pub strengths: Vec<String>,

    /// Weaknesses
    pub weaknesses: Vec<String>,

    /// Cost per 1M tokens (relative)
    pub relative_cost: f32,

    /// Speed (relative, higher = faster)
    pub relative_speed: f32,
}

impl ModelSelector {
    /// Create new model selector with default weights
    pub fn new() -> Self {
        let mut capabilities = HashMap::new();

        capabilities.insert(
            LlmProvider::Fast,
            ProviderCapabilities {
                max_context_tokens: 32_000,
                strengths: vec![
                    "Simple code generation".to_string(),
                    "Quick fixes".to_string(),
                ],
                weaknesses: vec![
                    "Limited reasoning".to_string(),
                    "Poor complex logic".to_string(),
                ],
                relative_cost: 0.1,
                relative_speed: 2.0,
            },
        );

        capabilities.insert(
            LlmProvider::Balanced,
            ProviderCapabilities {
                max_context_tokens: 64_000,
                strengths: vec![
                    "Good reasoning".to_string(),
                    "Balanced capability".to_string(),
                ],
                weaknesses: vec!["Not max capability".to_string()],
                relative_cost: 0.5,
                relative_speed: 1.0,
            },
        );

        capabilities.insert(
            LlmProvider::Powerful,
            ProviderCapabilities {
                max_context_tokens: 128_000,
                strengths: vec!["Strong reasoning".to_string(), "Complex logic".to_string()],
                weaknesses: vec!["Higher cost".to_string(), "Slower".to_string()],
                relative_cost: 1.0,
                relative_speed: 0.7,
            },
        );

        capabilities.insert(
            LlmProvider::MaxReasoning,
            ProviderCapabilities {
                max_context_tokens: 200_000,
                strengths: vec![
                    "Maximum reasoning".to_string(),
                    "Complex analysis".to_string(),
                ],
                weaknesses: vec!["Highest cost".to_string(), "Slowest".to_string()],
                relative_cost: 3.0,
                relative_speed: 0.4,
            },
        );

        Self {
            weights: SelectionWeights::default(),
            capabilities,
        }
    }

    /// Create with custom weights
    pub fn with_weights(weights: SelectionWeights) -> Self {
        let mut selector = Self::new();
        selector.weights = weights;
        selector
    }

    /// Select optimal model for a task
    pub fn select(&self, characteristics: &TaskCharacteristics) -> ModelSelection {
        let reasoning = Vec::new();
        let mut reasoning = reasoning;

        // Calculate base score for each provider
        let mut scores: HashMap<LlmProvider, f32> = HashMap::new();

        // Score based on entropy
        let entropy_score = match characteristics.entropy {
            TaskEntropy::Low => 0.1,
            TaskEntropy::Medium => 0.4,
            TaskEntropy::High => 0.7,
            TaskEntropy::Extreme => 1.0,
        };

        // Score based on risk
        let risk_score = match characteristics.risk_level {
            TaskRiskLevel::Low => 0.0,
            TaskRiskLevel::Medium => 0.3,
            TaskRiskLevel::High => 0.6,
            TaskRiskLevel::Critical => 1.0,
        };

        // Complexity score
        let complexity_score = (characteristics.file_count as f32 / 10.0).min(1.0) * 0.4
            + (characteristics.subtask_count as f32 / 20.0).min(1.0) * 0.3
            + if characteristics.is_cross_module {
                0.3
            } else {
                0.0
            };

        // High invariant density = simpler task
        let invariant_penalty = 1.0 - characteristics.invariant_density;

        // Calculate total score for each provider
        for provider in [
            LlmProvider::Fast,
            LlmProvider::Balanced,
            LlmProvider::Powerful,
            LlmProvider::MaxReasoning,
        ] {
            let capability_level = match provider {
                LlmProvider::Fast => 0.2,
                LlmProvider::Balanced => 0.5,
                LlmProvider::Powerful => 0.8,
                LlmProvider::MaxReasoning => 1.0,
            };

            // Score = how well capability matches task requirements
            let required_capability = entropy_score * self.weights.entropy_weight
                + risk_score * self.weights.risk_weight
                + complexity_score * self.weights.complexity_weight
                + invariant_penalty * self.weights.invariant_density_weight
                + if characteristics.is_cross_module {
                    0.2
                } else {
                    0.0
                };

            let score = 1.0 - (capability_level - required_capability).abs();

            scores.insert(provider, score.max(0.0));
        }

        // Sort by score
        let mut sorted: Vec<_> = scores.iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());

        let best_provider = *sorted[0].0;
        let best_score = *sorted[0].1;

        // Build reasoning
        reasoning.push(format!("Entropy: {:?}", characteristics.entropy));
        reasoning.push(format!("Risk: {:?}", characteristics.risk_level));
        reasoning.push(format!(
            "Files: {}, Subtasks: {}",
            characteristics.file_count, characteristics.subtask_count
        ));
        reasoning.push(format!(
            "Invariant density: {:.2}",
            characteristics.invariant_density
        ));

        if characteristics.is_cross_module {
            reasoning.push("Cross-module task detected".to_string());
        }

        // Calculate confidence
        let confidence = if sorted.len() > 1 {
            (best_score - sorted[1].1).min(1.0).max(0.0)
        } else {
            best_score
        };

        // Build alternatives
        let alternatives: Vec<(LlmProvider, f32)> = sorted[1..]
            .iter()
            .map(|(p, s)| (**p, **s))
            .take(2)
            .collect();

        // Recommendations based on provider capabilities
        let capabilities = self.capabilities.get(&best_provider).unwrap();
        let recommendations = capabilities.strengths.clone();

        ModelSelection {
            provider: best_provider,
            confidence,
            reasoning,
            alternatives,
            recommendations,
        }
    }

    /// Get capabilities for a provider
    pub fn get_capabilities(&self, provider: LlmProvider) -> Option<&ProviderCapabilities> {
        self.capabilities.get(&provider)
    }

    /// Get summary of all providers
    pub fn summary(&self) -> Vec<(LlmProvider, &ProviderCapabilities)> {
        let mut caps: Vec<_> = self.capabilities.iter().collect();
        caps.sort_by_key(|(p, _)| match p {
            LlmProvider::Fast => 0,
            LlmProvider::Balanced => 1,
            LlmProvider::Powerful => 2,
            LlmProvider::MaxReasoning => 3,
        });
        caps.into_iter().map(|(p, c)| (*p, c)).collect()
    }
}

impl Default for ModelSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create task characteristics from common patterns
impl TaskCharacteristics {
    /// Simple task (single file, low risk)
    pub fn simple() -> Self {
        Self {
            entropy: TaskEntropy::Low,
            risk_level: TaskRiskLevel::Low,
            file_count: 1,
            module_count: 1,
            api_call_count: 0,
            invariant_density: 0.8,
            is_cross_module: false,
            has_external_apis: false,
            subtask_count: 2,
        }
    }

    /// Moderate task
    pub fn moderate() -> Self {
        Self {
            entropy: TaskEntropy::Medium,
            risk_level: TaskRiskLevel::Medium,
            file_count: 3,
            module_count: 2,
            api_call_count: 3,
            invariant_density: 0.5,
            is_cross_module: true,
            has_external_apis: true,
            subtask_count: 8,
        }
    }

    /// Complex task
    pub fn complex() -> Self {
        Self {
            entropy: TaskEntropy::Extreme,       // Use Extreme for complex
            risk_level: TaskRiskLevel::Critical, // Critical for complex
            file_count: 15,
            module_count: 5,
            api_call_count: 15,
            invariant_density: 0.1, // Very low density
            is_cross_module: true,
            has_external_apis: true,
            subtask_count: 30,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_selector_new() {
        let selector = ModelSelector::new();
        let summary = selector.summary();
        assert_eq!(summary.len(), 4);
    }

    #[test]
    fn test_select_fast_for_simple_task() {
        let selector = ModelSelector::new();
        let characteristics = TaskCharacteristics::simple();

        let selection = selector.select(&characteristics);

        // Simple tasks should use fast model
        assert!(matches!(selection.provider, LlmProvider::Fast));
        assert!(selection.confidence > 0.0);
    }

    #[test]
    fn test_select_powerful_for_complex_task() {
        let selector = ModelSelector::new();
        let characteristics = TaskCharacteristics::complex();

        let selection = selector.select(&characteristics);

        // Complex tasks should use powerful model
        assert!(matches!(
            selection.provider,
            LlmProvider::Powerful | LlmProvider::MaxReasoning
        ));
    }

    #[test]
    fn test_select_balanced_for_moderate_task() {
        let selector = ModelSelector::new();
        let characteristics = TaskCharacteristics::moderate();

        let selection = selector.select(&characteristics);

        // Moderate tasks should use balanced
        assert!(matches!(selection.provider, LlmProvider::Balanced));
    }

    #[test]
    fn test_high_risk_requires_stronger_model() {
        let selector = ModelSelector::new();

        let low_risk = TaskCharacteristics {
            risk_level: TaskRiskLevel::Low,
            ..TaskCharacteristics::simple()
        };

        let high_risk = TaskCharacteristics {
            risk_level: TaskRiskLevel::Critical,
            ..TaskCharacteristics::simple()
        };

        let low_selection = selector.select(&low_risk);
        let high_selection = selector.select(&high_risk);

        // Higher risk should select stronger model
        let low_power = match low_selection.provider {
            LlmProvider::Fast => 0,
            LlmProvider::Balanced => 1,
            LlmProvider::Powerful => 2,
            LlmProvider::MaxReasoning => 3,
        };

        let high_power = match high_selection.provider {
            LlmProvider::Fast => 0,
            LlmProvider::Balanced => 1,
            LlmProvider::Powerful => 2,
            LlmProvider::MaxReasoning => 3,
        };

        assert!(high_power >= low_power);
    }

    #[test]
    fn test_high_invariant_density_uses_faster_model() {
        let selector = ModelSelector::new();

        let low_density = TaskCharacteristics {
            invariant_density: 0.1,
            ..TaskCharacteristics::simple()
        };

        let high_density = TaskCharacteristics {
            invariant_density: 0.9,
            ..TaskCharacteristics::simple()
        };

        let low_selection = selector.select(&low_density);
        let high_selection = selector.select(&high_density);

        // High invariant density should use faster model
        let low_power = match low_selection.provider {
            LlmProvider::Fast => 0,
            LlmProvider::Balanced => 1,
            LlmProvider::Powerful => 2,
            LlmProvider::MaxReasoning => 3,
        };

        let high_power = match high_selection.provider {
            LlmProvider::Fast => 0,
            LlmProvider::Balanced => 1,
            LlmProvider::Powerful => 2,
            LlmProvider::MaxReasoning => 3,
        };

        assert!(high_power <= low_power);
    }

    #[test]
    fn test_get_capabilities() {
        let selector = ModelSelector::new();

        let caps = selector.get_capabilities(LlmProvider::Balanced);
        assert!(caps.is_some());
        assert!(caps.unwrap().max_context_tokens > 0);
    }

    #[test]
    fn test_cross_module_task() {
        let selector = ModelSelector::new();

        let single = TaskCharacteristics {
            is_cross_module: false,
            ..TaskCharacteristics::moderate()
        };

        let cross = TaskCharacteristics {
            is_cross_module: true,
            ..TaskCharacteristics::moderate()
        };

        let single_selection = selector.select(&single);
        let cross_selection = selector.select(&cross);

        // Cross-module should use stronger model
        let single_power = match single_selection.provider {
            LlmProvider::Fast => 0,
            LlmProvider::Balanced => 1,
            LlmProvider::Powerful => 2,
            LlmProvider::MaxReasoning => 3,
        };

        let cross_power = match cross_selection.provider {
            LlmProvider::Fast => 0,
            LlmProvider::Balanced => 1,
            LlmProvider::Powerful => 2,
            LlmProvider::MaxReasoning => 3,
        };

        assert!(cross_power >= single_power);
    }

    #[test]
    fn test_task_characteristics_defaults() {
        let simple = TaskCharacteristics::simple();
        assert_eq!(simple.subtask_count, 2);
        assert_eq!(simple.file_count, 1);

        let complex = TaskCharacteristics::complex();
        assert_eq!(complex.subtask_count, 30);
        assert_eq!(complex.file_count, 15);
    }
}
