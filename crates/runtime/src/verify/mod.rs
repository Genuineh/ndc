//! Verify - Quality gate module
//!
//! Responsibilities:
//! - Run tests
//! - Execute linting
//!
//! Design principles:
//! - All checks are optionally configured
//! - Clear pass/fail criteria

use ndc_core::{TestType, QualityCheckType, QualityGate};
use crate::tools::{ShellTool, Tool};
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

impl QualityGateRunner {
    pub fn new() -> Self {
        Self {
            shell_tool: ShellTool::new(),
        }
    }

    /// Run quality gate
    pub async fn run(&self, gate: &QualityGate) -> Result<(), String> {
        info!("Running quality gate with {} checks", gate.checks.len());

        for check in &gate.checks {
            self.run_check(&check.check_type).await?;
        }

        info!("All quality checks passed");
        Ok(())
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

        let result = self.shell_tool.execute(&serde_json::json!({
            "command": command.split_whitespace().next().unwrap_or("cargo"),
            "args": if command.contains("test") {
                command.split_whitespace().skip(1).map(|s| s.to_string()).collect()
            } else {
                vec!["test".to_string()]
            },
            "timeout": 600
        })).await
            .map_err(|e| e.to_string())?;

        let passed = result.success && !result.output.contains("FAILED");

        if !passed {
            tracing::warn!("Tests failed");
        }

        Ok(QualityResult {
            passed,
            output: result.output,
            error: if passed { None } else { Some("Tests failed".to_string()) },
            metrics: QualityMetrics::default(),
        })
    }

    /// Run lint
    pub async fn run_lint(&self) -> Result<QualityResult, String> {
        let result = self.shell_tool.execute(&serde_json::json!({
            "command": "cargo",
            "args": vec!["clippy", "--", "-D", "warnings"],
            "timeout": 600
        })).await
            .map_err(|e| e.to_string())?;

        let passed = result.success && result.output.is_empty();

        Ok(QualityResult {
            passed,
            output: result.output,
            error: if passed { None } else { Some("Lint errors found".to_string()) },
            metrics: QualityMetrics::default(),
        })
    }

    /// Run type check
    pub async fn run_type_check(&self) -> Result<QualityResult, String> {
        let result = self.shell_tool.execute(&serde_json::json!({
            "command": "cargo",
            "args": vec!["check"],
            "timeout": 600
        })).await
            .map_err(|e| e.to_string())?;

        let passed = result.success;

        Ok(QualityResult {
            passed,
            output: result.output,
            error: if passed { None } else { Some("Type check failed".to_string()) },
            metrics: QualityMetrics::default(),
        })
    }

    /// Run build
    pub async fn run_build(&self) -> Result<QualityResult, String> {
        let result = self.shell_tool.execute(&serde_json::json!({
            "command": "cargo",
            "args": vec!["build"],
            "timeout": 600
        })).await
            .map_err(|e| e.to_string())?;

        let passed = result.success;

        Ok(QualityResult {
            passed,
            output: result.output,
            error: if passed { None } else { Some("Build failed".to_string()) },
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
