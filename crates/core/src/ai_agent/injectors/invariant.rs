//! Invariant Injector
//!
//! Injects Gold Memory invariants into Agent prompts
//!
//! Design:
//! - Extract relevant invariants based on current context
//! - Format as constraints for the LLM
//! - Priority-based ordering (critical first)

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Reuse core invariant priority type to keep a single semantic source.
pub type InvariantPriority = crate::InvariantPriority;

/// Gold Memory Invariant for agent injection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantEntry {
    /// Unique identifier
    pub id: String,

    /// Invariant description
    pub description: String,

    /// Context pattern that triggered this invariant
    pub pattern: Option<String>,

    /// Priority level
    pub priority: InvariantPriority,

    /// Source task that created this invariant
    pub source_task: Option<String>,

    /// When this was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// How many times this has been verified
    pub verification_count: u32,

    /// Whether this is currently active
    pub is_active: bool,
}

impl InvariantEntry {
    /// Create new entry
    pub fn new(id: String, description: String, priority: InvariantPriority) -> Self {
        Self {
            id,
            description,
            priority,
            pattern: None,
            source_task: None,
            created_at: chrono::Utc::now(),
            verification_count: 0,
            is_active: true,
        }
    }

    /// Mark as verified
    pub fn mark_verified(&mut self) {
        self.verification_count += 1;
    }
}

/// Invariant Injector configuration
#[derive(Debug, Clone)]
pub struct InvariantInjectorConfig {
    /// Maximum number of invariants to include
    pub max_invariants: usize,

    /// Include pattern context
    pub include_patterns: bool,

    /// Include source information
    pub include_source: bool,

    /// Only include active invariants
    pub active_only: bool,

    /// Minimum priority to include
    pub min_priority: InvariantPriority,
}

impl Default for InvariantInjectorConfig {
    fn default() -> Self {
        Self {
            max_invariants: 20,
            include_patterns: true,
            include_source: true,
            active_only: true,
            min_priority: InvariantPriority::Low,
        }
    }
}

/// Invariant Injector
#[derive(Debug, Clone)]
pub struct InvariantInjector {
    /// Known invariants
    invariants: Vec<InvariantEntry>,

    /// Configuration
    config: InvariantInjectorConfig,
}

impl InvariantInjector {
    /// Create new injector
    pub fn new(config: InvariantInjectorConfig) -> Self {
        Self {
            invariants: Vec::new(),
            config,
        }
    }

    /// Add an invariant
    pub fn add_invariant(&mut self, invariant: InvariantEntry) {
        self.invariants.push(invariant);
    }

    /// Add multiple invariants
    pub fn add_invariants(&mut self, invariants: Vec<InvariantEntry>) {
        self.invariants.extend(invariants);
    }

    /// Remove an invariant by ID
    pub fn remove_invariant(&mut self, id: &str) {
        self.invariants.retain(|i| i.id != id);
    }

    /// Get active invariants
    pub fn get_active(&self) -> Vec<&InvariantEntry> {
        self.invariants
            .iter()
            .filter(|i| i.is_active || !self.config.active_only)
            .collect()
    }

    /// Get relevant invariants based on context
    pub fn get_relevant(&self, context_patterns: &[String]) -> Vec<&InvariantEntry> {
        let mut relevant: Vec<&InvariantEntry> = self
            .invariants
            .iter()
            .filter(|i| i.is_active && i.priority >= self.config.min_priority)
            .collect();

        // Sort by priority (critical first)
        relevant.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Limit to max
        relevant.truncate(self.config.max_invariants);

        // Further filter by pattern match if patterns provided
        if !context_patterns.is_empty() {
            relevant.retain(|i| {
                if let Some(ref pattern) = i.pattern {
                    context_patterns
                        .iter()
                        .any(|p| p.contains(pattern) || pattern.contains(p))
                } else {
                    true
                }
            });
        }

        relevant
    }

    /// Inject as prompt text
    pub fn inject(&self, relevant_only: bool, patterns: &[String]) -> String {
        let invariants = if relevant_only {
            self.get_relevant(patterns)
        } else {
            self.invariants.iter().filter(|i| i.is_active).collect()
        };

        if invariants.is_empty() {
            return "=== INVARIANTS ===\n(no active invariants)\n=== END INVARIANTS ==="
                .to_string();
        }

        let mut lines = Vec::new();
        lines.push("=== GOLD MEMORY INVARIANTS ===".to_string());
        lines.push("⚠️  CRITICAL CONSTRAINTS - These patterns must never be repeated:".to_string());
        lines.push("".to_string());

        // Group by priority
        let critical: Vec<_> = invariants
            .iter()
            .filter(|i| i.priority == InvariantPriority::Critical)
            .collect();
        let high: Vec<_> = invariants
            .iter()
            .filter(|i| i.priority == InvariantPriority::High)
            .collect();
        let medium: Vec<_> = invariants
            .iter()
            .filter(|i| i.priority == InvariantPriority::Medium)
            .collect();
        let low: Vec<_> = invariants
            .iter()
            .filter(|i| i.priority == InvariantPriority::Low)
            .collect();

        // Critical (🔴)
        if !critical.is_empty() {
            lines.push("🔴 CRITICAL (Never repeat):".to_string());
            for inv in critical {
                lines.push(format!("  • {}", inv.description));
                if self.config.include_patterns
                    && let Some(ref pattern) = inv.pattern {
                        lines.push(format!("    Pattern: {}", pattern));
                    }
                if self.config.include_source
                    && let Some(ref task) = inv.source_task {
                        lines.push(format!("    From: {}", task));
                    }
            }
            lines.push("".to_string());
        }

        // High (🟠)
        if !high.is_empty() {
            lines.push("🟠 HIGH PRIORITY:".to_string());
            for inv in high {
                lines.push(format!("  • {}", inv.description));
            }
            lines.push("".to_string());
        }

        // Medium (🟡)
        if !medium.is_empty() {
            lines.push("🟡 MEDIUM PRIORITY:".to_string());
            for inv in medium {
                lines.push(format!("  • {}", inv.description));
            }
            lines.push("".to_string());
        }

        // Low (⚪)
        if !low.is_empty() {
            lines.push("⚪ LOW PRIORITY:".to_string());
            for inv in low {
                lines.push(format!("  • {}", inv.description));
            }
        }

        lines.push("".to_string());
        lines.push("=== END INVARIANTS ===".to_string());

        lines.join("\n")
    }

    /// Inject as structured data (for JSON-based prompts)
    pub fn inject_json(&self, patterns: &[String]) -> Value {
        let relevant = self.get_relevant(patterns);

        json!({
            "type": "invariants",
            "count": relevant.len(),
            "items": relevant.iter().map(|i| json!({
                "id": i.id,
                "description": i.description,
                "priority": i.priority,
                "pattern": i.pattern,
                "source": i.source_task,
            })).collect::<Vec<_>>(),
        })
    }

    /// Get statistics
    pub fn stats(&self) -> InvariantStats {
        InvariantStats {
            total: self.invariants.len(),
            active: self.invariants.iter().filter(|i| i.is_active).count(),
            critical: self
                .invariants
                .iter()
                .filter(|i| i.priority == InvariantPriority::Critical)
                .count(),
            high: self
                .invariants
                .iter()
                .filter(|i| i.priority == InvariantPriority::High)
                .count(),
            medium: self
                .invariants
                .iter()
                .filter(|i| i.priority == InvariantPriority::Medium)
                .count(),
            low: self
                .invariants
                .iter()
                .filter(|i| i.priority == InvariantPriority::Low)
                .count(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantStats {
    pub total: usize,
    pub active: usize,
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
}

impl Default for InvariantInjector {
    fn default() -> Self {
        Self::new(InvariantInjectorConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_active_respects_active_only() {
        let mut injector = InvariantInjector::default();
        let mut active = InvariantEntry::new(
            "active".to_string(),
            "active rule".to_string(),
            InvariantPriority::High,
        );
        active.is_active = true;
        let mut inactive = InvariantEntry::new(
            "inactive".to_string(),
            "inactive rule".to_string(),
            InvariantPriority::Low,
        );
        inactive.is_active = false;

        injector.add_invariant(active);
        injector.add_invariant(inactive);

        let active_only = injector.get_active();
        assert_eq!(active_only.len(), 1);
        assert_eq!(active_only[0].id, "active");
    }
}
