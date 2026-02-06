//! Verify - 质量门禁模块
//!
//! 职责：
//! - 运行测试
//! - 执行 linting
//!
//! 设计原则：
//! - 所有检查都是可选配置的
//! - 清晰的通过/失败标准
//! - 支持多种测试框架

use ndc_core::{TestType, QualityCheckType, Action};
use crate::tools::ShellTool;
use std::sync::Arc;
use tracing::{debug, info, warn, error};

/// 质量检查结果
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

/// 质量检查配置
#[derive(Debug, Clone)]
pub struct QualityCheck {
    pub check_type: QualityCheckType,
    pub enabled: bool,
    pub fail_on_warning: bool,
    pub custom_command: Option<String>,
}

impl Default for QualityCheck {
    fn default() -> Self {
        Self {
            check_type: QualityCheckType::Test,
            enabled: true,
            fail_on_warning: false,
            custom_command: None,
        }
    }
}

/// 质量门禁
#[derive(Debug, Clone)]
pub struct QualityGate {
    pub checks: Vec<QualityCheck>,
    pub strategy: GateStrategy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GateStrategy {
    /// 一个失败即停止
    FailFast,

    /// 全部通过
    AllMustPass,

    /// 加权评分
    Weighted,
}

impl Default for QualityGate {
    fn default() -> Self {
        Self {
            checks: vec![
                QualityCheck {
                    check_type: QualityCheckType::Test,
                    enabled: true,
                    fail_on_warning: false,
                    custom_command: None,
                },
            ],
            strategy: GateStrategy::FailFast,
        }
    }
}

/// 质量门禁运行器
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

    /// 运行质量门禁
    pub async fn run(
        &self,
        gate: &QualityGate,
        _context: &crate::ExecutionContext,
    ) -> Result<(), String> {
        info!("Running quality gate with {} checks", gate.checks.len());

        let mut results = Vec::new();

        for check in &gate.checks {
            if !check.enabled {
                debug!("Skipping disabled check: {:?}", check.check_type);
                continue;
            }

            let result = match check.check_type {
                QualityCheckType::Test => {
                    self.run_tests(&TestType::All).await
                }
                QualityCheckType::Lint => {
                    self.run_lint().await
                }
                QualityCheckType::TypeCheck => {
                    self.run_type_check().await
                }
                QualityCheckType::Build => {
                    self.run_build().await
                }
                QualityCheckType::Security => {
                    self.run_security_check().await
                }
                QualityCheckType::Custom(ref name) => {
                    self.run_custom_check(name).await
                }
            };

            match result {
                Ok(r) => results.push(r),
                Err(e) => {
                    error!("Quality check failed: {}", e);
                    return Err(e);
                }
            }
        }

        // 根据策略评估结果
        let all_passed = results.iter().all(|r| r.passed);
        let has_failures = results.iter().any(|r| !r.passed);

        match gate.strategy {
            GateStrategy::FailFast if has_failures => {
                let failed: Vec<_> = results.iter()
                    .filter(|r| !r.passed)
                    .map(|r| r.error.clone().unwrap_or_default())
                    .collect();
                return Err(format!("Quality gate failed: {:?}", failed));
            }
            GateStrategy::AllMustPass if !all_passed => {
                return Err("Not all quality checks passed".to_string());
            }
            _ => {}
        }

        if all_passed {
            info!("All quality checks passed");
        }

        Ok(())
    }

    /// 运行测试
    pub async fn run_tests(&self, test_type: &TestType) -> Result<QualityResult, String> {
        let start = std::time::Instant::now();

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

        let duration = start.elapsed().as_millis() as u64;

        // 解析测试结果
        let metrics = self.parse_test_output(&result.output);

        let passed = result.success && !result.output.contains("FAILED");

        if !passed {
            warn!("Tests failed");
        }

        Ok(QualityResult {
            passed,
            output: result.output,
            error: if passed { None } else { Some("Tests failed".to_string()) },
            metrics,
        })
    }

    /// 运行 lint
    pub async fn run_lint(&self) -> Result<QualityResult, String> {
        let start = std::time::Instant::now();

        let result = self.shell_tool.execute(&serde_json::json!({
            "command": "cargo",
            "args": vec!["clippy", "--", "-D", "warnings"],
            "timeout": 600
        })).await
            .map_err(|e| e.to_string())?;

        let duration = start.elapsed().as_millis() as u64;

        let passed = result.success && result.output.is_empty();
        let output = if result.error.is_some() {
            result.output
        } else {
            result.output
        };

        Ok(QualityResult {
            passed,
            output,
            error: if passed { None } else { Some("Lint errors found".to_string()) },
            metrics: QualityMetrics {
                tests_run: 0,
                tests_passed: 0,
                tests_failed: 0,
                duration_ms: duration,
            },
        })
    }

    /// 运行类型检查
    pub async fn run_type_check(&self) -> Result<QualityResult, String> {
        let start = std::time::Instant::now();

        let result = self.shell_tool.execute(&serde_json::json!({
            "command": "cargo",
            "args": vec!["check"],
            "timeout": 600
        })).await
            .map_err(|e| e.to_string())?;

        let duration = start.elapsed().as_millis() as u64;

        let passed = result.success;

        Ok(QualityResult {
            passed,
            output: result.output,
            error: if passed { None } else { Some("Type check failed".to_string()) },
            metrics: QualityMetrics {
                tests_run: 0,
                tests_passed: 0,
                tests_failed: 0,
                duration_ms: duration,
            },
        })
    }

    /// 运行构建
    pub async fn run_build(&self) -> Result<QualityResult, String> {
        let start = std::time::Instant::now();

        let result = self.shell_tool.execute(&serde_json::json!({
            "command": "cargo",
            "args": vec!["build"],
            "timeout": 600
        })).await
            .map_err(|e| e.to_string())?;

        let duration = start.elapsed().as_millis() as u64;

        let passed = result.success;

        Ok(QualityResult {
            passed,
            output: result.output,
            error: if passed { None } else { Some("Build failed".to_string()) },
            metrics: QualityMetrics {
                tests_run: 0,
                tests_passed: 0,
                tests_failed: 0,
                duration_ms: duration,
            },
        })
    }

    /// 运行安全检查
    pub async fn run_security_check(&self) -> Result<QualityResult, String> {
        // TODO: 实现安全检查
        Ok(QualityResult {
            passed: true,
            output: "Security check skipped (not implemented)".to_string(),
            error: None,
            metrics: QualityMetrics::default(),
        })
    }

    /// 运行自定义检查
    pub async fn run_custom_check(&self, _name: &str) -> Result<QualityResult, String> {
        // TODO: 实现自定义检查
        Ok(QualityResult {
            passed: true,
            output: "Custom check skipped (not implemented)".to_string(),
            error: None,
            metrics: QualityMetrics::default(),
        })
    }

    /// 解析测试输出
    fn parse_test_output(&self, output: &str) -> QualityMetrics {
        let mut metrics = QualityMetrics::default();

        // 简单解析：计算 run, passed, failed 数量
        if let Some(run_pos) = output.find("test result:") {
            let context = &output[run_pos..std::cmp::min(run_pos + 100, output.len())];
            if let Some(runs) = context.split_whitespace().next() {
                if let Ok(run_count) = runs.parse::<u32>() {
                    metrics.tests_run = run_count;
                }
            }
        }

        metrics
    }
}
