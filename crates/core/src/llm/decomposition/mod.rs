//! Decomposition lint module

pub mod lint;

pub use lint::{
    ActionType, Complexity, DecompositionLint, DependencyType, LintResult, LintSeverity,
    LintViolation, SubTask, TaskDecomposition, TaskDependency,
};
