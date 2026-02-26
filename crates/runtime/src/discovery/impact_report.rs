//! Discovery Phase - Impact Report
//!
//! Report generated from Discovery Phase analysis.
//! Captures what files/APIs will be affected by a task.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Impact Report - Discovery Phase Output
///
/// This report captures the scope and impact of a proposed task
/// before any execution begins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactReport {
    /// Unique report ID
    pub id: ImpactReportId,

    /// Task ID this report is for
    pub task_id: String,

    /// Original task request
    pub task_description: String,

    /// Files that will be read
    pub files_to_read: Vec<PathBuf>,

    /// Files that will be written/modified
    pub files_to_modify: Vec<PathBuf>,

    /// Files that will be created
    pub files_to_create: Vec<PathBuf>,

    /// Files that will be deleted
    pub files_to_delete: Vec<PathBuf>,

    /// Public API changes
    pub public_api_changes: Vec<ApiChange>,

    /// Git operations required
    pub git_operations: Vec<GitOperation>,

    /// Shell commands required
    pub shell_commands: Vec<ShellCommand>,

    /// External dependencies
    pub external_dependencies: Vec<ExternalDependency>,

    /// Impact scope
    pub scope: ImpactScope,

    /// Overall risk assessment
    pub risk_level: ndc_core::RiskLevel,

    /// Volatility score (0-1)
    pub volatility_score: f64,

    /// Estimated complexity
    pub complexity: Complexity,

    /// Time estimate
    pub estimated_duration_minutes: u32,

    /// Generated constraints
    pub generated_constraints: Option<String>, // JSON of HardConstraints

    /// Discovery findings
    pub findings: Vec<DiscoveryFinding>,

    /// Created at timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactReportId(pub String);

impl Default for ImpactReportId {
    fn default() -> Self {
        Self(format!("impact-{}", uuid::Uuid::new_v4()))
    }
}

impl fmt::Display for ImpactReportId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Impact scope levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImpactScope {
    /// Single file change
    Local,

    /// Multiple files in same module
    Module,

    /// Cross-module changes
    ModuleCrossing,

    /// Affects entire crate
    CrateWide,

    /// Affects multiple crates
    ProjectWide,
}

impl ImpactScope {
    /// Get from file count
    pub fn from_file_count(count: usize) -> Self {
        if count == 1 {
            ImpactScope::Local
        } else if count <= 3 {
            ImpactScope::Module
        } else if count <= 10 {
            ImpactScope::ModuleCrossing
        } else if count <= 50 {
            ImpactScope::CrateWide
        } else {
            ImpactScope::ProjectWide
        }
    }
}

/// API change description
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiChange {
    /// Change type
    pub change_type: ApiChangeType,

    /// Symbol name
    pub symbol: String,

    /// Symbol type
    pub symbol_type: String,

    /// File location
    pub file: PathBuf,

    /// Line number
    pub line: u32,

    /// Breaking change?
    pub is_breaking: bool,

    /// Description
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiChangeType {
    Add,
    Remove,
    Modify,
    Deprecate,
}

/// Git operation description
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitOperation {
    /// Operation type
    pub operation_type: GitOpType,

    /// Affected paths
    pub paths: Vec<PathBuf>,

    /// Branch name (if applicable)
    pub branch: Option<String>,

    /// Commit message (if committing)
    pub commit_message: Option<String>,

    /// Description
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GitOpType {
    Checkout,
    Branch,
    Commit,
    Push,
    Pull,
    Merge,
    Rebase,
    Reset,
}

/// Shell command description
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellCommand {
    /// Command to run
    pub command: String,

    /// Arguments
    pub args: Vec<String>,

    /// Working directory
    pub working_dir: Option<PathBuf>,

    /// Description
    pub description: String,

    /// Is dangerous?
    pub is_dangerous: bool,
}

/// External dependency change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalDependency {
    /// Dependency name
    pub name: String,

    /// Change type
    pub change_type: DepChangeType,

    /// Current version
    pub current_version: Option<String>,

    /// New version
    pub new_version: Option<String>,

    /// Reason
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DepChangeType {
    Add,
    Update,
    Remove,
}

/// Complexity estimation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Complexity {
    Trivial,
    Simple,
    Moderate,
    Complex,
    VeryComplex,
}

impl Complexity {
    /// Estimate from scope and volatility
    pub fn estimate(scope: ImpactScope, volatility: f64) -> Self {
        let base = match scope {
            ImpactScope::Local => 1,
            ImpactScope::Module => 2,
            ImpactScope::ModuleCrossing => 3,
            ImpactScope::CrateWide => 4,
            ImpactScope::ProjectWide => 5,
        };

        let volatility_factor = (volatility * 2.0) as u32;

        let score = base + volatility_factor;

        match score {
            0..=2 => Complexity::Trivial,
            3 => Complexity::Simple,
            4..=5 => Complexity::Moderate,
            6..=7 => Complexity::Complex,
            _ => Complexity::VeryComplex,
        }
    }
}

/// Discovery finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryFinding {
    /// Finding ID
    pub id: String,

    /// Category
    pub category: FindingCategory,

    /// Severity
    pub severity: ndc_core::RiskLevel,

    /// Title
    pub title: String,

    /// Description
    pub description: String,

    /// Affected files
    pub affected_files: Vec<PathBuf>,

    /// Recommendation
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FindingCategory {
    Architecture,
    Performance,
    Security,
    Compatibility,
    Testing,
    Documentation,
    Dependency,
    CodeQuality,
}

impl ImpactReport {
    /// Create new impact report
    pub fn new(task_id: String, task_description: String) -> Self {
        Self {
            id: ImpactReportId::default(),
            task_id,
            task_description,
            files_to_read: Vec::new(),
            files_to_modify: Vec::new(),
            files_to_create: Vec::new(),
            files_to_delete: Vec::new(),
            public_api_changes: Vec::new(),
            git_operations: Vec::new(),
            shell_commands: Vec::new(),
            external_dependencies: Vec::new(),
            scope: ImpactScope::Local,
            risk_level: ndc_core::RiskLevel::Low,
            volatility_score: 0.0,
            complexity: Complexity::Trivial,
            estimated_duration_minutes: 0,
            generated_constraints: None,
            findings: Vec::new(),
            created_at: chrono::Utc::now(),
        }
    }

    /// Add file to read
    pub fn add_file_to_read(&mut self, path: PathBuf) {
        self.files_to_read.push(path);
    }

    /// Add file to modify
    pub fn add_file_to_modify(&mut self, path: PathBuf) {
        self.files_to_modify.push(path);
    }

    /// Add file to create
    pub fn add_file_to_create(&mut self, path: PathBuf) {
        self.files_to_create.push(path);
    }

    /// Add finding
    pub fn add_finding(&mut self, finding: DiscoveryFinding) {
        self.findings.push(finding);
    }

    /// Calculate total affected files
    pub fn total_affected_files(&self) -> usize {
        self.files_to_modify.len() + self.files_to_create.len() + self.files_to_delete.len()
    }

    /// Calculate scope from affected files
    pub fn calculate_scope(&mut self) {
        let total = self.total_affected_files();
        self.scope = ImpactScope::from_file_count(total);
    }

    /// Calculate complexity
    pub fn calculate_complexity(&mut self) {
        self.complexity = Complexity::estimate(self.scope, self.volatility_score);
    }

    /// Check if high risk
    pub fn is_high_risk(&self) -> bool {
        self.risk_level == ndc_core::RiskLevel::High
            || self.risk_level == ndc_core::RiskLevel::Critical
    }

    /// Generate summary
    pub fn summary(&self) -> ImpactSummary {
        ImpactSummary {
            task_id: self.task_id.clone(),
            files_read: self.files_to_read.len(),
            files_modified: self.files_to_modify.len(),
            files_created: self.files_to_create.len(),
            files_deleted: self.files_to_delete.len(),
            scope: self.scope,
            risk_level: self.risk_level,
            complexity: self.complexity,
            duration_minutes: self.estimated_duration_minutes,
            finding_count: self.findings.len(),
        }
    }
}

/// Summary of impact report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactSummary {
    pub task_id: String,
    pub files_read: usize,
    pub files_modified: usize,
    pub files_created: usize,
    pub files_deleted: usize,
    pub scope: ImpactScope,
    pub risk_level: ndc_core::RiskLevel,
    pub complexity: Complexity,
    pub duration_minutes: u32,
    pub finding_count: usize,
}

impl fmt::Display for ImpactSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ImpactSummary for {}: {} files ({}R/{}M/{}C/{}D), scope: {:?}, risk: {:?}, complexity: {:?}, {} findings",
            self.task_id,
            self.files_read + self.files_modified + self.files_created + self.files_deleted,
            self.files_read,
            self.files_modified,
            self.files_created,
            self.files_deleted,
            self.scope,
            self.risk_level,
            self.complexity,
            self.finding_count,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_impact_report_new() {
        let report = ImpactReport::new("task-123".to_string(), "Add new feature".to_string());

        assert_eq!(report.task_id, "task-123");
        assert!(report.id.0.starts_with("impact-"));
        assert!(report.files_to_read.is_empty());
        assert!(report.findings.is_empty());
    }

    #[test]
    fn test_add_files() {
        let mut report = ImpactReport::new("task-123".to_string(), "Test".to_string());

        report.add_file_to_read(PathBuf::from("read.rs"));
        report.add_file_to_modify(PathBuf::from("modify.rs"));
        report.add_file_to_create(PathBuf::from("new.rs"));

        assert_eq!(report.files_to_read.len(), 1);
        assert_eq!(report.files_to_modify.len(), 1);
        assert_eq!(report.files_to_create.len(), 1);
    }

    #[test]
    fn test_scope_from_count() {
        assert_eq!(ImpactScope::from_file_count(1), ImpactScope::Local);
        assert_eq!(ImpactScope::from_file_count(3), ImpactScope::Module);
        assert_eq!(
            ImpactScope::from_file_count(10),
            ImpactScope::ModuleCrossing
        );
        assert_eq!(ImpactScope::from_file_count(50), ImpactScope::CrateWide);
        assert_eq!(ImpactScope::from_file_count(100), ImpactScope::ProjectWide);
    }

    #[test]
    fn test_complexity_estimate() {
        assert_eq!(
            Complexity::estimate(ImpactScope::Local, 0.1),
            Complexity::Trivial
        );
        assert_eq!(
            Complexity::estimate(ImpactScope::CrateWide, 0.8),
            Complexity::Moderate // base=4 + volatility_factor=1 = score=5 -> Moderate
        );
        assert_eq!(
            Complexity::estimate(ImpactScope::ProjectWide, 0.9),
            Complexity::Complex // base=5 + volatility_factor=1 = score=6 -> Complex
        );
    }

    #[test]
    fn test_summary_display() {
        let summary = ImpactSummary {
            task_id: "task-123".to_string(),
            files_read: 2,
            files_modified: 3,
            files_created: 1,
            files_deleted: 0,
            scope: ImpactScope::ModuleCrossing,
            risk_level: ndc_core::RiskLevel::Medium,
            complexity: Complexity::Moderate,
            duration_minutes: 30,
            finding_count: 2,
        };

        let output = summary.to_string();
        assert!(output.contains("task-123"));
        assert!(output.contains("ModuleCrossing"));
    }
}
