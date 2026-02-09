//! Decomposition lint module

pub mod lint;

pub use lint::{
    DecompositionLint,
    TaskDecomposition,
    SubTask,
    ActionType,
    Complexity,
    TaskDependency,
    DependencyType,
    LintResult,
    LintViolation,
    LintSeverity,
};
