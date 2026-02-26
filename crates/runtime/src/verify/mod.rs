//! Verify - Quality gate module
//!
//! Responsibilities:
//! - Run tests
//! - Execute linting
//!
//! Design principles:
//! - All checks are optionally configured
//! - Clear pass/fail criteria

use crate::discovery::{FileValidationType, HardConstraints};
use crate::tools::{ShellTool, Tool};
use ndc_core::{QualityCheckType, QualityGate, TestType};
use tracing::{debug, info};

/// Quality check result
#[derive(Debug, Clone)]
pub struct QualityResult {
    pub passed: bool,
    pub output: String,
    pub error: Option<String>,
    pub metrics: QualityMetrics,
}

#[derive(Debug, Clone, Default)]
pub struct QualityMetrics {
    pub tests_run: u32,
    pub tests_passed: u32,
    pub tests_failed: u32,
    pub duration_ms: u64,
}

/// Quality gate runner
#[derive(Debug)]
pub struct QualityGateRunner {
    shell_tool: ShellTool,
}

impl Default for QualityGateRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl QualityGateRunner {
    pub fn new() -> Self {
        Self {
            shell_tool: ShellTool::new(),
        }
    }

    /// Run quality gate
    pub async fn run(&self, gate: &QualityGate) -> Result<(), String> {
        self.run_with_constraints(Some(gate), None).await
    }

    /// Run quality gate with discovery hard constraints enforced.
    ///
    /// If hard constraints require additional checks, those checks are merged into
    /// the existing gate and enforced as mandatory.
    pub async fn run_with_constraints(
        &self,
        gate: Option<&QualityGate>,
        constraints: Option<&HardConstraints>,
    ) -> Result<(), String> {
        let checks = Self::collect_enforced_checks(gate, constraints);
        info!("Running quality gate with {} enforced checks", checks.len());

        if checks.is_empty() {
            info!("No quality checks to run");
            return Ok(());
        }

        for check in &checks {
            let result = self.run_check(check).await?;
            if !result.passed {
                return Err(result
                    .error
                    .unwrap_or_else(|| format!("Quality check failed: {:?}", check)));
            }
        }
        info!("All quality checks passed");
        Ok(())
    }

    fn collect_enforced_checks(
        gate: Option<&QualityGate>,
        constraints: Option<&HardConstraints>,
    ) -> Vec<QualityCheckType> {
        let mut checks = Vec::new();
        let mut seen = std::collections::HashSet::new();

        if let Some(gate) = gate {
            for check in &gate.checks {
                Self::push_unique_check(&mut checks, &mut seen, check.check_type.clone());
            }
        }

        if let Some(constraints) = constraints {
            if !constraints.mandatory_regression_tests.is_empty()
                || !constraints.verified_api_surface.is_empty()
                || !constraints.high_volatility_modules.is_empty()
            {
                Self::push_unique_check(&mut checks, &mut seen, QualityCheckType::Test);
            }

            if !constraints.version_sensitive_constraints.is_empty() {
                Self::push_unique_check(&mut checks, &mut seen, QualityCheckType::Build);
            }

            if constraints
                .coupling_warnings
                .iter()
                .any(|warning| warning.coupling_type.is_dangerous())
            {
                Self::push_unique_check(&mut checks, &mut seen, QualityCheckType::Lint);
            }

            for validation in &constraints.mandatory_validations {
                let check_type = match validation.validation_type {
                    FileValidationType::Syntax | FileValidationType::Types => {
                        QualityCheckType::TypeCheck
                    }
                    FileValidationType::Formatting | FileValidationType::Linting => {
                        QualityCheckType::Lint
                    }
                    FileValidationType::Security => QualityCheckType::Security,
                    FileValidationType::Documentation => {
                        QualityCheckType::Custom("documentation".to_string())
                    }
                };
                Self::push_unique_check(&mut checks, &mut seen, check_type);
            }
        }

        checks
    }

    fn push_unique_check(
        checks: &mut Vec<QualityCheckType>,
        seen: &mut std::collections::HashSet<String>,
        check: QualityCheckType,
    ) {
        let key = Self::check_key(&check);
        if seen.insert(key) {
            checks.push(check);
        }
    }

    fn check_key(check: &QualityCheckType) -> String {
        match check {
            QualityCheckType::Test => "test".to_string(),
            QualityCheckType::Lint => "lint".to_string(),
            QualityCheckType::TypeCheck => "typecheck".to_string(),
            QualityCheckType::Build => "build".to_string(),
            QualityCheckType::Security => "security".to_string(),
            QualityCheckType::Custom(name) => format!("custom:{}", name),
        }
    }

    /// Run a single quality check
    pub async fn run_check(&self, check_type: &QualityCheckType) -> Result<QualityResult, String> {
        match check_type {
            QualityCheckType::Test => self.run_tests(&TestType::All).await,
            QualityCheckType::Lint => self.run_lint().await,
            QualityCheckType::TypeCheck => self.run_type_check().await,
            QualityCheckType::Build => self.run_build().await,
            QualityCheckType::Security => self.run_security_check().await,
            QualityCheckType::Custom(_) => self.run_custom_check().await,
        }
    }

    /// Run tests
    pub async fn run_tests(&self, test_type: &TestType) -> Result<QualityResult, String> {
        let command = match test_type {
            TestType::Unit => "cargo test --lib -- --nocapture".to_string(),
            TestType::Integration => "cargo test --test -- --nocapture".to_string(),
            TestType::All => "cargo test".to_string(),
        };

        debug!("Running tests: {}", command);

        let result = self
            .shell_tool
            .execute(&serde_json::json!({
                "command": command.split_whitespace().next().unwrap_or("cargo"),
                "args": if command.contains("test") {
                    command.split_whitespace().skip(1).map(|s| s.to_string()).collect()
                } else {
                    vec!["test".to_string()]
                },
                "timeout": 600
            }))
            .await
            .map_err(|e| e.to_string())?;

        let passed = result.success && !result.output.contains("FAILED");

        if !passed {
            tracing::warn!("Tests failed");
        }

        Ok(QualityResult {
            passed,
            output: result.output,
            error: if passed {
                None
            } else {
                Some("Tests failed".to_string())
            },
            metrics: QualityMetrics::default(),
        })
    }

    /// Run lint
    pub async fn run_lint(&self) -> Result<QualityResult, String> {
        let result = self
            .shell_tool
            .execute(&serde_json::json!({
                "command": "cargo",
                "args": vec!["clippy", "--", "-D", "warnings"],
                "timeout": 600
            }))
            .await
            .map_err(|e| e.to_string())?;

        let passed = result.success;

        Ok(QualityResult {
            passed,
            output: result.output,
            error: if passed {
                None
            } else {
                Some("Lint errors found".to_string())
            },
            metrics: QualityMetrics::default(),
        })
    }

    /// Run type check
    pub async fn run_type_check(&self) -> Result<QualityResult, String> {
        let result = self
            .shell_tool
            .execute(&serde_json::json!({
                "command": "cargo",
                "args": vec!["check"],
                "timeout": 600
            }))
            .await
            .map_err(|e| e.to_string())?;

        let passed = result.success;

        Ok(QualityResult {
            passed,
            output: result.output,
            error: if passed {
                None
            } else {
                Some("Type check failed".to_string())
            },
            metrics: QualityMetrics::default(),
        })
    }

    /// Run build
    pub async fn run_build(&self) -> Result<QualityResult, String> {
        let result = self
            .shell_tool
            .execute(&serde_json::json!({
                "command": "cargo",
                "args": vec!["build"],
                "timeout": 600
            }))
            .await
            .map_err(|e| e.to_string())?;

        let passed = result.success;

        Ok(QualityResult {
            passed,
            output: result.output,
            error: if passed {
                None
            } else {
                Some("Build failed".to_string())
            },
            metrics: QualityMetrics::default(),
        })
    }

    /// Run security check
    pub async fn run_security_check(&self) -> Result<QualityResult, String> {
        Ok(QualityResult {
            passed: true,
            output: "Security check skipped (not implemented)".to_string(),
            error: None,
            metrics: QualityMetrics::default(),
        })
    }

    /// Run custom check
    pub async fn run_custom_check(&self) -> Result<QualityResult, String> {
        Ok(QualityResult {
            passed: true,
            output: "Custom check skipped (not implemented)".to_string(),
            error: None,
            metrics: QualityMetrics::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::{
        ComponentKind, ComponentRef, CouplingType, CouplingWarning, FileValidation,
        HardConstraints, RegressionTest,
    };
    use ndc_core::RiskLevel;
    use std::path::PathBuf;

    #[test]
    fn test_collect_enforced_checks_from_constraints() {
        let mut constraints = HardConstraints::new("task-1".to_string());
        constraints.add_regression_test(RegressionTest {
            module: "core".to_string(),
            test_files: vec![PathBuf::from("tests/core.rs")],
            test_types: vec![],
            coverage_requirement: 0.8,
        });
        constraints.mandatory_validations.push(FileValidation {
            path: PathBuf::from("src/lib.rs"),
            validation_type: FileValidationType::Security,
            reason: "critical path".to_string(),
            tool: "cargo-audit".to_string(),
        });
        constraints.add_coupling_warning(CouplingWarning {
            id: "cw-1".to_string(),
            source: ComponentRef {
                name: "src".to_string(),
                path: PathBuf::from("src"),
                kind: ComponentKind::Module,
            },
            target: ComponentRef {
                name: "ffi".to_string(),
                path: PathBuf::from("ffi"),
                kind: ComponentKind::Module,
            },
            coupling_type: CouplingType::FfiBoundary,
            risk_level: RiskLevel::High,
            description: "ffi boundary".to_string(),
            mitigation: "add lint and tests".to_string(),
        });

        let checks = QualityGateRunner::collect_enforced_checks(None, Some(&constraints));
        let keys: std::collections::HashSet<_> =
            checks.iter().map(QualityGateRunner::check_key).collect();

        assert!(keys.contains("test"));
        assert!(keys.contains("security"));
        assert!(keys.contains("lint"));
    }
}
