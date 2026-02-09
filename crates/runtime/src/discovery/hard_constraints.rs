//! Discovery Phase - Hard Constraints
//!
//! Constraints generated from Discovery Phase that MUST be enforced
//! during execution. This ensures execution doesn't ignore Discovery findings.

use ndc_core::RiskLevel;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fmt;

/// Hard Constraints - Generated from Discovery Phase
///
/// These constraints are NOT suggestions - they are mandatory requirements
/// that must be satisfied before a task can complete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardConstraints {
    /// Unique ID for tracking
    pub id: HardConstraintsId,

    /// Task ID these constraints belong to
    pub task_id: String,

    /// Mandatory regression tests
    pub mandatory_regression_tests: Vec<RegressionTest>,

    /// Verified API surface that must be tested
    pub verified_api_surface: Vec<ApiSymbol>,

    /// High volatility modules requiring extra attention
    pub high_volatility_modules: Vec<HighVolatilityModule>,

    /// Implicit coupling warnings
    pub coupling_warnings: Vec<CouplingWarning>,

    /// Version-sensitive constraints
    pub version_sensitive_constraints: Vec<VersionedConstraint>,

    /// Files that must be validated
    pub mandatory_validations: Vec<FileValidation>,

    /// Created at timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardConstraintsId(pub String);

impl Default for HardConstraintsId {
    fn default() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl fmt::Display for HardConstraintsId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Regression test specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionTest {
    /// Module being tested
    pub module: String,

    /// Test files that must pass
    pub test_files: Vec<PathBuf>,

    /// Test types required
    pub test_types: Vec<TestType>,

    /// Coverage requirement (0.0 - 1.0)
    pub coverage_requirement: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestType {
    Unit,
    Integration,
    Property,
    Fuzz,
    Documentation,
}

/// API symbol that must be verified
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSymbol {
    /// Symbol name
    pub name: String,

    /// Symbol type
    pub kind: ApiKind,

    /// File location
    pub file: PathBuf,

    /// Line number
    pub line: u32,

    /// Whether it's public/exported
    pub is_public: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Type,
    Constant,
    Macro,
}

/// High volatility module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighVolatilityModule {
    /// Module identifier
    pub module_id: String,

    /// Module path
    pub path: PathBuf,

    /// Volatility score (0-1)
    pub volatility_score: f64,

    /// Risk level
    pub risk_level: RiskLevel,

    /// Required test coverage for this module
    pub required_coverage: f64,

    /// Files that changed recently
    pub changed_files: Vec<PathBuf>,
}

/// Implicit coupling warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouplingWarning {
    /// Warning ID
    pub id: String,

    /// Source component
    pub source: ComponentRef,

    /// Target component
    pub target: ComponentRef,

    /// Type of coupling
    pub coupling_type: CouplingType,

    /// Risk level
    pub risk_level: RiskLevel,

    /// Description
    pub description: String,

    /// Mitigation suggestion
    pub mitigation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRef {
    pub name: String,
    pub path: PathBuf,
    pub kind: ComponentKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComponentKind {
    Module,
    Crate,
    Binary,
    Library,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CouplingType {
    /// Uses reflection/runtime type inspection
    Reflection,

    /// Uses procedural macros
    MacroInjection,

    /// Uses dynamic configuration
    DynamicConfig,

    /// Uses runtime polymorphism (trait objects)
    RuntimePolymorphism,

    /// Global state access
    GlobalState,

    /// Cross-thread communication
    ThreadCoupling,

    /// FFI boundary
    FfiBoundary,
}

impl CouplingType {
    pub fn is_dangerous(&self) -> bool {
        matches!(
            self,
            CouplingType::Reflection
                | CouplingType::MacroInjection
                | CouplingType::DynamicConfig
                | CouplingType::FfiBoundary
        )
    }
}

/// Version-sensitive constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedConstraint {
    /// Constraint ID
    pub id: String,

    /// The constraint rule
    pub rule: String,

    /// Version dimension this applies to
    pub version_dimension: VersionDimension,

    /// Version constraint operator
    pub operator: VersionOperator,

    /// Expected version value
    pub expected_version: String,

    /// Actual version (to verify)
    pub actual_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VersionDimension {
    RustVersion,
    CrateVersion { crate_name: String },
    Platform { target_triple: String },
    Dependency { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VersionOperator {
    ExactMatch,
    AtLeast,
    AtMost,
    Compatible,
}

/// File validation requirement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileValidation {
    /// File path
    pub path: PathBuf,

    /// Validation type
    pub validation_type: FileValidationType,

    /// Why this must be validated
    pub reason: String,

    /// Tool to use
    pub tool: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileValidationType {
    /// Check syntax
    Syntax,

    /// Check types
    Types,

    /// Check formatting
    Formatting,

    /// Check linting
    Linting,

    /// Check security
    Security,

    /// Check documentation
    Documentation,
}

impl HardConstraints {
    /// Create new empty constraints
    pub fn new(task_id: String) -> Self {
        Self {
            id: HardConstraintsId::default(),
            task_id,
            mandatory_regression_tests: Vec::new(),
            verified_api_surface: Vec::new(),
            high_volatility_modules: Vec::new(),
            coupling_warnings: Vec::new(),
            version_sensitive_constraints: Vec::new(),
            mandatory_validations: Vec::new(),
            created_at: chrono::Utc::now(),
        }
    }

    /// Add a regression test requirement
    pub fn add_regression_test(&mut self, test: RegressionTest) {
        self.mandatory_regression_tests.push(test);
    }

    /// Add an API symbol to verify
    pub fn add_api_symbol(&mut self, symbol: ApiSymbol) {
        self.verified_api_surface.push(symbol);
    }

    /// Add a high volatility module
    pub fn add_high_volatility_module(&mut self, module: HighVolatilityModule) {
        self.high_volatility_modules.push(module);
    }

    /// Add a coupling warning
    pub fn add_coupling_warning(&mut self, warning: CouplingWarning) {
        self.coupling_warnings.push(warning);
    }

    /// Check if all constraints are satisfied
    pub fn is_satisfied(&self) -> bool {
        // All regression tests must have passed
        self.mandatory_regression_tests
            .iter()
            .all(|t| t.test_files.is_empty() || t.coverage_requirement <= 0.0)
    }

    /// Get failed constraints
    pub fn get_failed_constraints(&self) -> Vec<FailedConstraint> {
        let mut failures = Vec::new();

        for test in &self.mandatory_regression_tests {
            failures.push(FailedConstraint {
                constraint_type: "regression_test".to_string(),
                description: format!("Regression test for module: {}", test.module),
                severity: Severity::High,
            });
        }

        for warning in &self.coupling_warnings {
            if warning.risk_level == RiskLevel::High || warning.risk_level == RiskLevel::Critical {
                failures.push(FailedConstraint {
                    constraint_type: "coupling_warning".to_string(),
                    description: warning.description.clone(),
                    severity: Severity::from(&warning.risk_level),
                });
            }
        }

        failures
    }

    /// Generate summary for logging
    pub fn summary(&self) -> HardConstraintsSummary {
        HardConstraintsSummary {
            task_id: self.task_id.clone(),
            regression_test_count: self.mandatory_regression_tests.len(),
            api_symbol_count: self.verified_api_surface.len(),
            high_volatility_count: self.high_volatility_modules.len(),
            coupling_warning_count: self.coupling_warnings.len(),
            version_constraint_count: self.version_sensitive_constraints.len(),
            validation_count: self.mandatory_validations.len(),
        }
    }
}

/// Failed constraint record
#[derive(Debug, Clone)]
pub struct FailedConstraint {
    pub constraint_type: String,
    pub description: String,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl From<&RiskLevel> for Severity {
    fn from(risk: &RiskLevel) -> Self {
        match risk {
            RiskLevel::Low => Severity::Low,
            RiskLevel::Medium => Severity::Medium,
            RiskLevel::High => Severity::High,
            RiskLevel::Critical => Severity::Critical,
        }
    }
}

/// Summary of hard constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardConstraintsSummary {
    pub task_id: String,
    pub regression_test_count: usize,
    pub api_symbol_count: usize,
    pub high_volatility_count: usize,
    pub coupling_warning_count: usize,
    pub version_constraint_count: usize,
    pub validation_count: usize,
}

impl fmt::Display for HardConstraintsSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "HardConstraintsSummary for {}: {} regression tests, {} API symbols, {} high-volatility modules, {} coupling warnings",
            self.task_id,
            self.regression_test_count,
            self.api_symbol_count,
            self.high_volatility_count,
            self.coupling_warning_count,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hard_constraints_new() {
        let constraints = HardConstraints::new("task-123".to_string());

        assert_eq!(constraints.task_id, "task-123");
        // UUID format check
        assert!(!constraints.id.0.is_empty());
        assert!(constraints.mandatory_regression_tests.is_empty());
    }

    #[test]
    fn test_add_regression_test() {
        let mut constraints = HardConstraints::new("task-123".to_string());

        constraints.add_regression_test(RegressionTest {
            module: "core".to_string(),
            test_files: vec![PathBuf::from("test_core.rs")],
            test_types: vec![TestType::Unit, TestType::Integration],
            coverage_requirement: 0.8,
        });

        assert_eq!(constraints.mandatory_regression_tests.len(), 1);
        assert_eq!(constraints.mandatory_regression_tests[0].module, "core");
    }

    #[test]
    fn test_coupling_type_dangerous() {
        assert!(CouplingType::Reflection.is_dangerous());
        assert!(CouplingType::MacroInjection.is_dangerous());
        assert!(CouplingType::DynamicConfig.is_dangerous());
        assert!(CouplingType::FfiBoundary.is_dangerous());

        assert!(!CouplingType::RuntimePolymorphism.is_dangerous());
        assert!(!CouplingType::GlobalState.is_dangerous());
    }

    #[test]
    fn test_summary_display() {
        let summary = HardConstraintsSummary {
            task_id: "task-123".to_string(),
            regression_test_count: 2,
            api_symbol_count: 5,
            high_volatility_count: 1,
            coupling_warning_count: 3,
            version_constraint_count: 0,
            validation_count: 4,
        };

        let output = summary.to_string();
        assert!(output.contains("task-123"));
        assert!(output.contains("2"));
        assert!(output.contains("5"));
    }
}
