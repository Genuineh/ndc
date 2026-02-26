//! Simple Token Counter
//!
//! Approximate token counting for LLM models.
//! Uses character-based estimation with model-specific multipliers.

use super::*;
use std::collections::HashMap;

/// Token counting error
#[derive(Debug, Error)]
pub enum TokenCountError {
    #[error("Unknown model: {model}")]
    UnknownModel { model: String },
}

/// Simple token counter using character-based estimation
#[derive(Debug, Clone)]
pub struct SimpleTokenCounter {
    // Model-specific token-per-character ratios
    model_ratios: HashMap<String, f32>,
}

impl SimpleTokenCounter {
    /// Create a new token counter with default models
    pub fn new() -> Self {
        let mut counter = Self {
            model_ratios: HashMap::new(),
        };

        // GPT-4 family (approximately 4 chars per token)
        for model in &[
            "gpt-4",
            "gpt-4-turbo",
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-3.5-turbo",
        ] {
            counter.model_ratios.insert(model.to_string(), 0.25);
        }

        // Claude family (approximately 4 chars per token)
        for model in &[
            "claude-opus-4",
            "claude-sonnet-4",
            "claude-haiku-4",
            "claude-3-5-sonnet",
            "claude-3-opus",
            "claude-3-haiku",
        ] {
            counter.model_ratios.insert(model.to_string(), 0.25);
        }

        // Default ratio
        counter.model_ratios.insert("default".to_string(), 0.25);

        counter
    }

    /// Get token-per-character ratio for a model
    fn get_ratio(&self, model: &str) -> f32 {
        // Try exact match first
        if let Some(ratio) = self.model_ratios.get(model) {
            return *ratio;
        }

        // Try partial match for model families
        for (model_pattern, ratio) in &self.model_ratios {
            if model.contains(model_pattern) || model_pattern.contains(model) {
                return *ratio;
            }
        }

        0.25 // Default ratio
    }
}

impl Default for SimpleTokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TokenCounter for SimpleTokenCounter {
    fn count_messages(&self, messages: &[Message], model: &str) -> usize {
        let _ratio = self.get_ratio(model);
        let mut total = 0;

        // Count tokens for each message
        for message in messages {
            // Add overhead for role (approximately 1 token for role + 2 for formatting)
            total += 3;

            // Count content tokens
            total += self.count_text(&message.content, model);

            // Add tool call tokens if present
            if let Some(calls) = &message.tool_calls {
                for call in calls {
                    total += self.count_text(&call.function.name, model);
                    total += self.count_text(&call.function.arguments, model);
                }
            }
        }

        total
    }

    fn count_text(&self, text: &str, _model: &str) -> usize {
        // Simple estimation: average 4 characters per token
        (text.len() / 4).max(1)
    }

    fn get_max_tokens(&self, model: &str) -> usize {
        match model {
            m if m.contains("gpt-4") && m.contains("turbo") => 128_000,
            m if m.contains("gpt-4o") => 128_000,
            m if m.contains("gpt-4") => 8_192,
            m if m.contains("gpt-3.5-turbo") => 16_385,
            m if m.contains("claude-opus") => 200_000,
            m if m.contains("claude-sonnet") => 200_000,
            m if m.contains("claude-haiku") => 200_000,
            m if m.contains("claude-3") => 200_000,
            _ => 8_192,
        }
    }
}

/// Count tokens using tiktoken-style encoding (placeholder)
pub fn estimate_tokens(text: &str, _encoding_name: &str) -> usize {
    // Placeholder implementation
    // In production, use actual tiktoken or equivalent
    (text.len() / 4).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_token_counter() {
        let counter = SimpleTokenCounter::new();

        // Test text counting
        let tokens = counter.count_text("Hello world", "gpt-4");
        assert!(tokens > 0);
        assert!(tokens <= 10);
    }

    #[test]
    fn test_message_counting() {
        let counter = SimpleTokenCounter::new();

        let messages = vec![
            Message {
                role: MessageRole::System,
                content: "You are a helpful assistant.".to_string(),
                name: None,
                tool_calls: None,
            },
            Message {
                role: MessageRole::User,
                content: "Hello!".to_string(),
                name: None,
                tool_calls: None,
            },
        ];

        let tokens = counter.count_messages(&messages, "gpt-4");
        assert!(tokens > 0);
    }

    #[test]
    fn test_max_tokens() {
        let counter = SimpleTokenCounter::new();

        assert_eq!(counter.get_max_tokens("gpt-4"), 8192);
        assert_eq!(counter.get_max_tokens("gpt-4o"), 128000);
        assert!(counter.get_max_tokens("claude-opus-4") >= 100000);
    }
}
