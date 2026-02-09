//! Working Memory - Hierarchical Execution Context
//!
//! Provides structured context for task execution with three levels:
//! - Abstract: Historical failure patterns and root cause analysis
//! - Raw: Current step information (files, APIs)
//! - Hard: Invariants from Gold Memory
//!
//! Core principle: Abstract(History) + Raw(Current) + Hard(Invariants)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Scope identifier for this working memory
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubTaskId(pub String);

impl Default for SubTaskId {
    fn default() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for SubTaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Working Memory - Hierarchical execution context
#[derive(Debug, Clone)]
pub struct WorkingMemory {
    /// Scope: current subtask
    pub scope: SubTaskId,

    /// Level 1: Abstract History
    /// Records WHY it failed, not just WHAT failed
    pub abstract_history: AbstractHistory,

    /// Level 2: Raw Current
    /// Raw information for current step
    pub raw_current: RawCurrent,

    /// Level 3: Hard Invariants
    /// From Gold Memory - must never violate
    pub hard_invariants: Vec<VersionedInvariant>,
}

/// Abstract History - Failure pattern analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbstractHistory {
    /// Failure patterns (Why it failed)
    pub failure_patterns: Vec<FailurePattern>,

    /// Root cause summary
    pub root_cause_summary: Option<String>,

    /// Attempt count
    pub attempt_count: u8,

    /// Trajectory state
    pub trajectory_state: TrajectoryState,
}

/// Failure pattern for tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePattern {
    /// Error type
    pub error_type: String,

    /// Error message
    pub message: String,

    /// File where error occurred
    pub file: Option<PathBuf>,

    /// Line number
    pub line: Option<u32>,

    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl FailurePattern {
    /// Check if two patterns have the same root cause
    pub fn same_root_cause(&self, other: &FailurePattern) -> bool {
        self.error_type == other.error_type && self.message == other.message
    }
}

/// Trajectory state - where are we in the execution?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrajectoryState {
    /// Making progress
    Progressing { steps_since_last_failure: u8 },

    /// Stuck in a loop
    Cycling { repeated_pattern: String },

    /// Completely stuck
    Stuck { last_error: String },
}

/// Raw Current - Current step information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawCurrent {
    /// Active files being worked on
    pub active_files: Vec<PathBuf>,

    /// API surface in use
    pub api_surface: Vec<ApiSurface>,

    /// Current step context
    pub current_step_context: Option<StepContext>,
}

/// API surface entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSurface {
    /// Symbol name
    pub name: String,

    /// Kind of symbol
    pub kind: ApiKind,

    /// File location
    pub file: PathBuf,

    /// Line number
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiKind {
    Function,
    Struct,
    Enum,
    Trait,
    Type,
    Constant,
}

/// Step context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepContext {
    /// Step description
    pub description: String,

    /// Step index
    pub step_index: u32,

    /// Expected output
    pub expected_output: Option<String>,
}

/// Versioned Invariant - from Gold Memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedInvariant {
    pub id: String,
    pub rule: String,
    pub scope: String,
    pub priority: InvariantPriority,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub ttl_days: Option<u32>,
    pub version_tags: Vec<VersionTag>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum InvariantPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionTag {
    pub dimension: String,
    pub value: String,
    pub operator: VersionOperator,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum VersionOperator {
    Exact,
    AtLeast,
    AtMost,
}

/// LLM Context -精简 context for LLM consumption
#[derive(Debug, Clone)]
pub struct LlmContext {
    /// History summary
    pub history: String,

    /// Current files
    pub current_files: Vec<PathBuf>,

    /// APIs in use
    pub apis: Vec<ApiSurface>,

    /// Hard invariants
    pub invariants: String,
}

impl WorkingMemory {
    /// Generate working memory from discovery + history + invariants
    pub fn generate(
        scope: SubTaskId,
        abstract_history: Option<AbstractHistory>,
        raw_current: RawCurrent,
        hard_invariants: Vec<VersionedInvariant>,
    ) -> Self {
        Self {
            scope,
            abstract_history: abstract_history.unwrap_or_else(|| AbstractHistory {
                failure_patterns: Vec::new(),
                root_cause_summary: None,
                attempt_count: 0,
                trajectory_state: TrajectoryState::Progressing { steps_since_last_failure: 0 },
            }),
            raw_current,
            hard_invariants,
        }
    }

    /// Generate concise context for LLM
    pub fn concise_context_for_llm(&self) -> LlmContext {
        let history_summary = format!(
            "Attempts: {}, Status: {:?}, Root Cause: {:?}",
            self.abstract_history.attempt_count,
            self.abstract_history.trajectory_state,
            self.abstract_history.root_cause_summary
        );

        let invariants_summary = self.hard_invariants
            .iter()
            .map(|i| format!("MUST: {}", i.rule))
            .collect::<Vec<_>>()
            .join("; ");

        LlmContext {
            history: history_summary,
            current_files: self.raw_current.active_files.clone(),
            apis: self.raw_current.api_surface.clone(),
            invariants: invariants_summary,
        }
    }

    /// Record a failure
    pub fn record_failure(&mut self, pattern: FailurePattern) {
        self.abstract_history.attempt_count += 1;

        // Save message before moving
        let error_message = pattern.message.clone();
        self.abstract_history.failure_patterns.push(pattern);

        // Update trajectory
        if self.abstract_history.attempt_count > 3 {
            self.abstract_history.trajectory_state = TrajectoryState::Cycling {
                repeated_pattern: "Repeated failures detected".to_string(),
            };
        } else {
            self.abstract_history.trajectory_state = TrajectoryState::Stuck {
                last_error: error_message,
            };
        }
    }

    /// Record a success
    pub fn record_success(&mut self) {
        self.abstract_history.trajectory_state = TrajectoryState::Progressing {
            steps_since_last_failure: self.abstract_history.attempt_count,
        };
    }

    /// Check if stuck in a cycle
    pub fn is_cycling(&self) -> bool {
        matches!(
            self.abstract_history.trajectory_state,
            TrajectoryState::Cycling { .. }
        )
    }

    /// Check if stuck completely
    pub fn is_stuck(&self) -> bool {
        matches!(
            self.abstract_history.trajectory_state,
            TrajectoryState::Stuck { .. }
        )
    }
}

impl AbstractHistory {
    /// Detect if stuck in a cycle
    pub fn detect_cycle(&self) -> bool {
        // Cycle if: >3 attempts AND >2 failures with same root cause
        self.attempt_count > 3 && self.failure_patterns
            .windows(3)
            .any(|window| {
                if window.len() >= 3 {
                    window[0].same_root_cause(&window[1]) &&
                    window[1].same_root_cause(&window[2])
                } else {
                    false
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_working_memory_new() {
        let raw = RawCurrent {
            active_files: vec![PathBuf::from("test.rs")],
            api_surface: Vec::new(),
            current_step_context: None,
        };

        let wm = WorkingMemory::generate(
            SubTaskId::default(),
            None,
            raw,
            Vec::new(),
        );

        assert_eq!(wm.abstract_history.attempt_count, 0);
        assert!(matches!(wm.abstract_history.trajectory_state,
            TrajectoryState::Progressing { .. }));
    }

    #[test]
    fn test_record_failure() {
        let raw = RawCurrent {
            active_files: vec![],
            api_surface: Vec::new(),
            current_step_context: None,
        };

        let mut wm = WorkingMemory::generate(
            SubTaskId::default(),
            None,
            raw,
            Vec::new(),
        );

        wm.record_failure(FailurePattern {
            error_type: "TypeError".to_string(),
            message: "Cannot read property".to_string(),
            file: Some(PathBuf::from("test.rs")),
            line: Some(10),
            timestamp: chrono::Utc::now(),
        });

        assert_eq!(wm.abstract_history.attempt_count, 1);
        assert!(wm.is_stuck());
    }

    #[test]
    fn test_record_success() {
        let raw = RawCurrent {
            active_files: vec![],
            api_surface: Vec::new(),
            current_step_context: None,
        };

        let mut wm = WorkingMemory::generate(
            SubTaskId::default(),
            None,
            raw,
            Vec::new(),
        );

        wm.record_failure(FailurePattern {
            error_type: "Test".to_string(),
            message: "Error".to_string(),
            file: None,
            line: None,
            timestamp: chrono::Utc::now(),
        });

        wm.record_success();

        assert!(matches!(wm.abstract_history.trajectory_state,
            TrajectoryState::Progressing { .. }));
    }

    #[test]
    fn test_llm_context() {
        let raw = RawCurrent {
            active_files: vec![PathBuf::from("src/lib.rs")],
            api_surface: vec![ApiSurface {
                name: "test_fn".to_string(),
                kind: ApiKind::Function,
                file: PathBuf::from("src/lib.rs"),
                line: 5,
            }],
            current_step_context: None,
        };

        let invariants = vec![VersionedInvariant {
            id: "inv-1".to_string(),
            rule: "Always validate input".to_string(),
            scope: "global".to_string(),
            priority: InvariantPriority::High,
            created_at: chrono::Utc::now(),
            ttl_days: Some(90),
            version_tags: Vec::new(),
        }];

        let wm = WorkingMemory::generate(
            SubTaskId::default(),
            None,
            raw,
            invariants,
        );

        let ctx = wm.concise_context_for_llm();

        assert!(ctx.history.contains("Attempts: 0"));
        assert!(ctx.invariants.contains("Always validate input"));
        assert_eq!(ctx.current_files.len(), 1);
    }

    #[test]
    fn test_failure_pattern_same_root_cause() {
        let p1 = FailurePattern {
            error_type: "TypeError".to_string(),
            message: "Cannot read".to_string(),
            file: None,
            line: None,
            timestamp: chrono::Utc::now(),
        };

        let p2 = FailurePattern {
            error_type: "TypeError".to_string(),
            message: "Cannot read".to_string(),
            file: None,
            line: None,
            timestamp: chrono::Utc::now(),
        };

        assert!(p1.same_root_cause(&p2));
    }

    #[test]
    fn test_detect_cycle() {
        let history = AbstractHistory {
            failure_patterns: vec![
                FailurePattern {
                    error_type: "TypeError".to_string(),
                    message: "Same error".to_string(),
                    file: None,
                    line: None,
                    timestamp: chrono::Utc::now(),
                },
                FailurePattern {
                    error_type: "TypeError".to_string(),
                    message: "Same error".to_string(),
                    file: None,
                    line: None,
                    timestamp: chrono::Utc::now(),
                },
                FailurePattern {
                    error_type: "TypeError".to_string(),
                    message: "Same error".to_string(),
                    file: None,
                    line: None,
                    timestamp: chrono::Utc::now(),
                },
            ],
            root_cause_summary: None,
            attempt_count: 4,
            trajectory_state: TrajectoryState::Stuck { last_error: "Same error".to_string() },
        };

        assert!(history.detect_cycle());
    }
}
