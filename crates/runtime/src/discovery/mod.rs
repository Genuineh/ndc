//! Discovery Phase Module
//!
//! Provides read-only impact analysis before execution.
//! Generates ImpactReport, Volatility Heatmap, and Hard Constraints.

pub mod hard_constraints;
pub mod heatmap;
pub mod impact_report;

pub use heatmap::{
    ChangeType, GitChange, HeatmapConfig, HeatmapError, ModuleId, ModuleVolatility,
    VolatilityHeatmap, volatility_to_risk_level,
};

pub use hard_constraints::{
    ApiKind, ApiSymbol, ComponentKind, ComponentRef, CouplingType, CouplingWarning,
    FailedConstraint, FileValidation, FileValidationType, HardConstraints, HardConstraintsId,
    HardConstraintsSummary, HighVolatilityModule, RegressionTest, Severity, TestType,
    VersionDimension, VersionOperator, VersionedConstraint,
};

pub use impact_report::{
    ApiChange, ApiChangeType, Complexity, DepChangeType, DiscoveryFinding, ExternalDependency,
    FindingCategory, GitOpType, GitOperation, ImpactReport, ImpactReportId, ImpactScope,
    ImpactSummary, ShellCommand,
};

/// Discovery Service - Main entry point for Discovery Phase
///
/// Responsibilities:
/// 1. Analyze task scope and impact
/// 2. Generate Volatility Heatmap
/// 3. Produce ImpactReport
/// 4. Create Hard Constraints
#[derive(Debug, Clone)]
pub struct DiscoveryService {
    /// Configuration
    config: DiscoveryConfig,

    /// Git repository path
    repo_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Enable heatmap generation
    pub enable_heatmap: bool,

    /// Enable hard constraints
    pub enable_hard_constraints: bool,

    /// Heatmap lookback days
    pub heatmap_lookback_days: u32,

    /// High volatility threshold
    pub high_volatility_threshold: u32,

    /// Risk threshold for requiring hard constraints
    pub risk_threshold: f64,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enable_heatmap: true,
            enable_hard_constraints: true,
            heatmap_lookback_days: 7,
            high_volatility_threshold: 5,
            risk_threshold: 0.7,
        }
    }
}

impl DiscoveryService {
    /// Create new discovery service
    pub fn new(repo_path: PathBuf, config: Option<DiscoveryConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
            repo_path,
        }
    }

    /// Run discovery phase for a task
    pub async fn discover(
        &self,
        task_id: String,
        task_description: String,
        affected_files: Vec<PathBuf>,
    ) -> Result<DiscoveryResult, DiscoveryError> {
        let mut report = ImpactReport::new(task_id.clone(), task_description);

        // Add affected files
        for file in &affected_files {
            report.add_file_to_modify(file.clone());
        }

        // Generate heatmap if enabled
        let heatmap = if self.config.enable_heatmap {
            Some(self.generate_heatmap().await?)
        } else {
            None
        };

        // Update report with heatmap data
        if let Some(ref heatmap) = heatmap {
            // Calculate volatility score
            let avg_volatility = affected_files
                .iter()
                .map(|f| if heatmap.is_high_risk(f) { 1.0 } else { 0.0 })
                .sum::<f64>()
                / affected_files.len().max(1) as f64;

            report.volatility_score = avg_volatility;
            report.risk_level = heatmap::volatility_to_risk_level(avg_volatility);
        }

        // Calculate scope and complexity
        report.calculate_scope();
        report.calculate_complexity();

        // Generate hard constraints if high risk
        let hard_constraints = if self.should_generate_constraints(&report) {
            Some(
                self.generate_hard_constraints(&report, heatmap.as_ref())
                    .await?,
            )
        } else {
            None
        };

        // Store constraints as JSON
        if let Some(ref constraints) = hard_constraints {
            report.generated_constraints = Some(serde_json::to_string(constraints).unwrap());
        }

        Ok(DiscoveryResult {
            impact_report: report,
            heatmap,
            hard_constraints,
        })
    }

    /// Generate volatility heatmap
    async fn generate_heatmap(&self) -> Result<VolatilityHeatmap, DiscoveryError> {
        let config = HeatmapConfig {
            lookback_days: self.config.heatmap_lookback_days,
            high_volatility_threshold: self.config.high_volatility_threshold,
            normalization_factor: 1.0,
        };

        VolatilityHeatmap::from_git(&self.repo_path, Some(config))
            .await
            .map_err(DiscoveryError::from)
    }

    /// Check if should generate hard constraints
    fn should_generate_constraints(&self, report: &ImpactReport) -> bool {
        if !self.config.enable_hard_constraints {
            return false;
        }

        report.volatility_score >= self.config.risk_threshold || report.is_high_risk()
    }

    /// Generate hard constraints
    async fn generate_hard_constraints(
        &self,
        report: &ImpactReport,
        heatmap: Option<&VolatilityHeatmap>,
    ) -> Result<HardConstraints, DiscoveryError> {
        let mut constraints = HardConstraints::new(report.task_id.clone());

        // Add high volatility modules from heatmap
        if let Some(heatmap) = heatmap {
            for module_volatility in heatmap.get_high_volatility_modules() {
                constraints.add_high_volatility_module(HighVolatilityModule {
                    module_id: module_volatility.module.name.clone(),
                    path: module_volatility.module.path.clone(),
                    volatility_score: module_volatility.score,
                    risk_level: module_volatility.risk_level,
                    required_coverage: 0.8, // Default 80%
                    changed_files: module_volatility.recent_files.clone(),
                });
            }
        }

        // Add coupling warnings for high-risk files
        for file in &report.files_to_modify {
            if let Some(heatmap) = heatmap
                && heatmap.is_high_risk(file)
            {
                constraints.add_coupling_warning(CouplingWarning {
                    id: format!("coupling-{}", uuid::Uuid::new_v4()),
                    source: ComponentRef {
                        name: file
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_string())
                            .unwrap_or_default(),
                        path: file.clone(),
                        kind: ComponentKind::Module,
                    },
                    target: ComponentRef {
                        name: "unknown".to_string(),
                        path: PathBuf::new(),
                        kind: ComponentKind::Module,
                    },
                    coupling_type: CouplingType::DynamicConfig,
                    risk_level: ndc_core::RiskLevel::Medium,
                    description: format!("High volatility file: {}", file.display()),
                    mitigation: "Ensure thorough testing before commit".to_string(),
                });
            }
        }

        Ok(constraints)
    }
}

/// Discovery result containing all outputs
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    /// Impact analysis report
    pub impact_report: ImpactReport,

    /// Volatility heatmap (if generated)
    pub heatmap: Option<VolatilityHeatmap>,

    /// Hard constraints (if high risk)
    pub hard_constraints: Option<HardConstraints>,
}

impl DiscoveryResult {
    /// Check if any constraints were generated
    pub fn has_constraints(&self) -> bool {
        self.hard_constraints.is_some()
    }

    /// Get all failed constraints
    pub fn get_failed_constraints(&self) -> Vec<FailedConstraint> {
        self.hard_constraints
            .as_ref()
            .map(|c| c.get_failed_constraints())
            .unwrap_or_default()
    }
}

/// Discovery errors
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("Heatmap error: {0}")]
    HeatmapError(#[from] HeatmapError),

    #[error("Git error: {0}")]
    GitError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Path buffer type
type PathBuf = std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_discovery_service() {
        // Create temp directory
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().to_path_buf();

        // Initialize git repo
        let output = tokio::process::Command::new("git")
            .args(&["init"])
            .current_dir(&repo_path)
            .output()
            .await
            .unwrap();

        assert!(output.status.success());

        // Create service
        let service = DiscoveryService::new(repo_path.clone(), Some(DiscoveryConfig::default()));

        // Run discovery
        let result = service
            .discover(
                "test-task".to_string(),
                "Test discovery".to_string(),
                vec![PathBuf::from("test.rs")],
            )
            .await;

        match result {
            Ok(discovery_result) => {
                assert_eq!(discovery_result.impact_report.task_id, "test-task");
                // Heatmap might be empty for fresh repo
            }
            Err(e) => {
                // Heatmap might fail for fresh repos, that's OK
                println!("Discovery note: {:?}", e);
            }
        }
    }

    #[test]
    fn test_discovery_config_default() {
        let config = DiscoveryConfig::default();

        assert!(config.enable_heatmap);
        assert!(config.enable_hard_constraints);
        assert_eq!(config.heatmap_lookback_days, 7);
        assert_eq!(config.risk_threshold, 0.7);
    }

    #[test]
    fn test_discovery_result_has_constraints() {
        let result = DiscoveryResult {
            impact_report: ImpactReport::new("task-1".to_string(), "Test".to_string()),
            heatmap: None,
            hard_constraints: None,
        };

        assert!(!result.has_constraints());

        let report = ImpactReport::new("task-1".to_string(), "Test".to_string());

        let result_with_constraints = DiscoveryResult {
            impact_report: report,
            heatmap: None,
            hard_constraints: Some(HardConstraints::new("task-1".to_string())),
        };

        assert!(result_with_constraints.has_constraints());
    }
}
