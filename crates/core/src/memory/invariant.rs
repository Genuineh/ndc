//! Invariant Gold Memory - Persistent storage for human-corrected invariants
//!
//! "同一个坑填过一次，永远不会再掉进去"
//!
//! Core flow:
//! 1. Human corrects an error -> FailureTaxonomy::HumanCorrection
//! 2. Abstract to FormalConstraint
//! 3. Inject into Gold Memory
//! 4. Propagate to:
//!    - Future WorkingMemory
//!    - Decomposition Validator
//!    - ModelSelector (high risk flag)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for an invariant
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantId(pub String);

impl Default for InvariantId {
    fn default() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for InvariantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Priority levels for invariants (Higher = more important)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum InvariantPriority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

/// How an invariant was derived
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InvariantSource {
    /// Derived from human correction
    HumanCorrection {
        /// Who made the correction
        corrector_id: String,
        /// Original error that was corrected
        original_error: String,
        /// The fix that was applied
        fix_description: String,
    },
    /// Discovered through automated testing
    AutomatedTest {
        /// Test name that discovered this
        test_name: String,
        /// The failing test output
        test_output: String,
    },
    /// Inferred from system analysis
    SystemInference {
        /// Analysis method used
        analysis_method: String,
        /// Evidence supporting this invariant
        evidence: Vec<String>,
    },
    /// Transferred from parent task lineage
    LineageTransfer {
        /// Source task ID
        source_task_id: String,
        /// Validation count from source
        validation_count: u32,
    },
}

/// Scope where invariant applies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantScope {
    /// Scope type
    pub scope_type: ScopeType,
    /// Pattern to match (regex or exact match)
    pub pattern: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScopeType {
    /// Applies to all tasks
    Global,
    /// Applies to tasks matching a pattern
    TaskPattern,
    /// Applies to specific file patterns
    FilePattern,
    /// Applies to specific modules
    Module,
    /// Applies to API calls matching pattern
    ApiPattern,
}

/// Version compatibility for invariants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionConstraint {
    /// Dimension (e.g., "rust-version", "dependency-xxx")
    pub dimension: String,
    /// Operator for comparison
    pub operator: VersionOperator,
    /// Value to compare against
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionOperator {
    Exact,
    AtLeast,
    AtMost,
    Range,
}

/// Gold Memory Invariant - The "Golden" knowledge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldInvariant {
    /// Unique ID
    pub id: InvariantId,

    /// The formal constraint rule
    pub rule: String,

    /// Human-readable description
    pub description: String,

    /// Source of this invariant
    pub source: InvariantSource,

    /// Where this invariant applies
    pub scope: InvariantScope,

    /// Priority level
    pub priority: InvariantPriority,

    /// Version constraints
    pub version_constraints: Vec<VersionConstraint>,

    /// Categories/tags
    pub tags: Vec<String>,

    /// How many times validated
    pub validation_count: u32,

    /// How many times violated
    pub violation_count: u32,

    /// Last validation timestamp
    pub last_validated: chrono::DateTime<chrono::Utc>,

    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Is this invariant active?
    pub is_active: bool,
}

impl GoldInvariant {
    /// Create a new invariant from human correction
    pub fn from_human_correction(
        corrector_id: String,
        original_error: String,
        fix_description: String,
        rule: String,
        description: String,
    ) -> Self {
        Self {
            id: InvariantId::default(),
            rule,
            description,
            source: InvariantSource::HumanCorrection {
                corrector_id,
                original_error,
                fix_description,
            },
            scope: InvariantScope {
                scope_type: ScopeType::Global,
                pattern: ".*".to_string(),
            },
            priority: InvariantPriority::High,
            version_constraints: Vec::new(),
            tags: vec!["human-corrected".to_string()],
            validation_count: 0,
            violation_count: 0,
            last_validated: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
            is_active: true,
        }
    }

    /// Check if this invariant is applicable to a context
    pub fn is_applicable(&self, _context: &InvariantContext) -> bool {
        // For now, all invariants are applicable
        // In full implementation, would check scope patterns
        true
    }

    /// Mark as validated
    pub fn mark_validated(&mut self) {
        self.validation_count += 1;
        self.last_validated = chrono::Utc::now();
    }

    /// Mark as violated
    pub fn mark_violated(&mut self) {
        self.violation_count += 1;
        // If violated too many times, may need review
        if self.violation_count > self.validation_count {
            self.priority = InvariantPriority::Critical;
        }
    }
}

/// Context for checking invariant applicability
#[derive(Debug, Clone)]
pub struct InvariantContext {
    /// Task description
    pub task_description: String,

    /// Files involved
    pub files: Vec<std::path::PathBuf>,

    /// Modules involved
    pub modules: Vec<String>,

    /// API calls involved
    pub api_calls: Vec<String>,

    /// Minimum priority to consider
    pub min_priority: Option<InvariantPriority>,
}

/// Query for searching invariants
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InvariantQuery {
    /// Filter by priority
    pub priority: Option<InvariantPriority>,

    /// Filter by scope type
    pub scope_type: Option<ScopeType>,

    /// Filter by tags
    pub tags: Vec<String>,

    /// Only active invariants
    pub only_active: bool,

    /// Minimum validation count
    pub min_validation_count: u32,
}

/// Gold Memory - Persistent storage for invariants
#[derive(Debug, Clone)]
pub struct GoldMemory {
    /// All stored invariants
    invariants: Vec<GoldInvariant>,

    /// Index by scope for fast lookup
    scope_index: HashMap<ScopeType, Vec<usize>>,

    /// Index by priority
    priority_index: HashMap<InvariantPriority, Vec<usize>>,

    /// Index by tags
    tag_index: HashMap<String, Vec<usize>>,
}

impl GoldMemory {
    /// Create new empty gold memory
    pub fn new() -> Self {
        Self {
            invariants: Vec::new(),
            scope_index: HashMap::new(),
            priority_index: HashMap::new(),
            tag_index: HashMap::new(),
        }
    }

    /// Add an invariant
    pub fn add_invariant(&mut self, invariant: GoldInvariant) -> InvariantId {
        let id = invariant.id.clone();
        let idx = self.invariants.len();

        self.invariants.push(invariant);

        // Update indexes
        let inv = &self.invariants[idx];
        self.scope_index.entry(inv.scope.scope_type).or_default().push(idx);
        self.priority_index.entry(inv.priority).or_default().push(idx);
        for tag in &inv.tags {
            self.tag_index.entry(tag.clone()).or_default().push(idx);
        }

        id
    }

    /// Get invariant by ID
    pub fn get(&self, id: &InvariantId) -> Option<&GoldInvariant> {
        self.invariants.iter().find(|i| &i.id == id)
    }

    /// Get mutable invariant by ID
    pub fn get_mut(&mut self, id: &InvariantId) -> Option<&mut GoldInvariant> {
        self.invariants.iter_mut().find(|i| &i.id == id)
    }

    /// Find applicable invariants for a context
    pub fn find_applicable(&self, context: &InvariantContext) -> Vec<&GoldInvariant> {
        self.invariants.iter()
            .filter(|inv| inv.is_applicable(context))
            .collect()
    }

    /// Query invariants
    pub fn query(&self, query: &InvariantQuery) -> Vec<&GoldInvariant> {
        self.invariants.iter()
            .filter(|inv| {
                if query.only_active && !inv.is_active {
                    return false;
                }
                if let Some(priority) = query.priority {
                    if inv.priority != priority {
                        return false;
                    }
                }
                if let Some(scope_type) = query.scope_type {
                    if inv.scope.scope_type != scope_type {
                        return false;
                    }
                }
                if !query.tags.is_empty() {
                    if !query.tags.iter().any(|t| inv.tags.contains(t)) {
                        return false;
                    }
                }
                if inv.validation_count < query.min_validation_count {
                    return false;
                }
                true
            })
            .collect()
    }

    /// Get summary statistics
    pub fn summary(&self) -> GoldMemorySummary {
        let mut by_priority: HashMap<InvariantPriority, usize> = HashMap::new();
        let mut by_scope: HashMap<ScopeType, usize> = HashMap::new();

        for inv in &self.invariants {
            *by_priority.entry(inv.priority).or_insert(0) += 1;
            *by_scope.entry(inv.scope.scope_type).or_insert(0) += 1;
        }

        GoldMemorySummary {
            total_invariants: self.invariants.len(),
            active_invariants: self.invariants.iter().filter(|i| i.is_active).count(),
            by_priority,
            by_scope,
            total_validations: self.invariants.iter().map(|i| i.validation_count).sum(),
            total_violations: self.invariants.iter().map(|i| i.violation_count).sum(),
        }
    }
}

/// Summary statistics for gold memory
#[derive(Debug, Clone)]
pub struct GoldMemorySummary {
    pub total_invariants: usize,
    pub active_invariants: usize,
    pub by_priority: HashMap<InvariantPriority, usize>,
    pub by_scope: HashMap<ScopeType, usize>,
    pub total_validations: u32,
    pub total_violations: u32,
}

/// Gold Memory Service - Higher-level operations
#[derive(Debug, Clone)]
pub struct GoldMemoryService {
    /// Storage for invariants
    gold_memory: GoldMemory,
}

impl GoldMemoryService {
    /// Create new service
    pub fn new() -> Self {
        Self {
            gold_memory: GoldMemory::new(),
        }
    }

    /// Create invariant from human correction
    pub fn create_from_human_correction(
        &mut self,
        corrector_id: String,
        original_error: String,
        fix_description: String,
        _task_context: InvariantContext,
    ) -> InvariantId {
        let error_clone = original_error.clone();
        let fix_clone = fix_description.clone();

        let rule = format!(
            "Human correction for: {}. Rule: {}",
            error_clone,
            fix_clone
        );

        let invariant = GoldInvariant::from_human_correction(
            corrector_id,
            original_error,
            fix_description,
            rule,
            format!("Corrected: {}", error_clone),
        );

        self.gold_memory.add_invariant(invariant)
    }

    /// Get invariants for decomposition validation
    pub fn get_invariants_for_decomposition(&self, task_id: &str) -> Vec<&GoldInvariant> {
        let context = InvariantContext {
            task_description: task_id.to_string(),
            files: Vec::new(),
            modules: Vec::new(),
            api_calls: Vec::new(),
            min_priority: Some(InvariantPriority::High),
        };

        self.gold_memory.find_applicable(&context)
    }

    /// Get invariants for working memory
    pub fn get_invariants_for_working_memory(&self, context: &InvariantContext) -> Vec<&GoldInvariant> {
        self.gold_memory.find_applicable(context)
    }

    /// Validate an action against invariants
    pub fn validate_action(&self, context: &InvariantContext) -> ValidationResult {
        let applicable = self.gold_memory.find_applicable(context);

        // Simplified: assume no violations in this placeholder implementation
        let violations: Vec<&GoldInvariant> = Vec::new();

        let passed = violations.is_empty();

        ValidationResult {
            passed,
            violations: violations.iter().map(|i| i.rule.clone()).collect(),
            applicable_count: applicable.len(),
        }
    }

    /// Mark invariant as validated
    pub fn mark_validated(&mut self, id: &InvariantId) -> bool {
        self.gold_memory.get_mut(id).map(|inv| {
            inv.mark_validated();
            true
        }).unwrap_or(false)
    }

    /// Get mutable invariant by ID and mark as violated
    pub fn mark_violated(&mut self, id: &InvariantId) -> bool {
        self.gold_memory.get_mut(id).map(|inv| {
            inv.mark_violated();
            true
        }).unwrap_or(false)
    }

    /// Get summary
    pub fn summary(&self) -> GoldMemorySummary {
        self.gold_memory.summary()
    }
}

/// Result of validating an action
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Did validation pass?
    pub passed: bool,

    /// Rules that were violated
    pub violations: Vec<String>,

    /// How many invariants were applicable
    pub applicable_count: usize,
}

impl Default for GoldMemoryService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gold_memory_new() {
        let gm = GoldMemory::new();
        let summary = gm.summary();
        assert_eq!(summary.total_invariants, 0);
    }

    #[test]
    fn test_add_invariant() {
        let mut gm = GoldMemory::new();
        let invariant = GoldInvariant::from_human_correction(
            "user-1".to_string(),
            "Null pointer exception".to_string(),
            "Added null check".to_string(),
            "ALWAYS validate inputs".to_string(),
            "Check for null before use".to_string(),
        );

        let id = gm.add_invariant(invariant);
        assert_eq!(gm.summary().total_invariants, 1);
        assert!(gm.get(&id).is_some());
    }

    #[test]
    fn test_find_applicable() {
        let mut gm = GoldMemory::new();
        let invariant = GoldInvariant::from_human_correction(
            "user-1".to_string(),
            "Error".to_string(),
            "Fix".to_string(),
            "Test rule".to_string(),
            "Test description".to_string(),
        );

        gm.add_invariant(invariant);

        let context = InvariantContext {
            task_description: "test task".to_string(),
            files: vec![],
            modules: vec![],
            api_calls: vec![],
            min_priority: None,
        };

        let applicable = gm.find_applicable(&context);
        assert!(!applicable.is_empty());
    }

    #[test]
    fn test_query_by_priority() {
        let mut gm = GoldMemory::new();

        for i in 0..5 {
            let mut inv = GoldInvariant::from_human_correction(
                "user-1".to_string(),
                format!("Error {}", i),
                "Fix".to_string(),
                format!("Rule {}", i),
                format!("Description {}", i),
            );
            inv.priority = if i < 3 { InvariantPriority::High } else { InvariantPriority::Low };
            gm.add_invariant(inv);
        }

        let query = InvariantQuery {
            priority: Some(InvariantPriority::High),
            ..Default::default()
        };

        let results = gm.query(&query);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_mark_validated() {
        let mut service = GoldMemoryService::new();
        let invariant = GoldInvariant::from_human_correction(
            "user-1".to_string(),
            "Error".to_string(),
            "Fix".to_string(),
            "Rule".to_string(),
            "Description".to_string(),
        );

        let id = service.gold_memory.add_invariant(invariant);
        assert_eq!(service.gold_memory.get(&id).unwrap().validation_count, 0);

        service.mark_validated(&id);
        assert_eq!(service.gold_memory.get(&id).unwrap().validation_count, 1);
    }

    #[test]
    fn test_summary() {
        let mut gm = GoldMemory::new();

        for i in 0..3 {
            let inv = GoldInvariant::from_human_correction(
                "user-1".to_string(),
                format!("Error {}", i),
                "Fix".to_string(),
                format!("Rule {}", i),
                format!("Description {}", i),
            );
            gm.add_invariant(inv);
        }

        let summary = gm.summary();
        assert_eq!(summary.total_invariants, 3);
        assert_eq!(summary.active_invariants, 3);
    }

    #[test]
    fn test_invariant_from_human_correction() {
        let inv = GoldInvariant::from_human_correction(
            "admin".to_string(),
            "File permission error".to_string(),
            "Changed to 644".to_string(),
            "Use 644 for source files".to_string(),
            "File permissions must be 644".to_string(),
        );

        assert_eq!(inv.priority, InvariantPriority::High);
        assert!(inv.is_active);
        assert!(matches!(inv.source, InvariantSource::HumanCorrection { .. }));
        assert!(inv.tags.contains(&"human-corrected".to_string()));
    }
}
