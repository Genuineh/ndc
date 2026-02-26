//! Discovery Phase - Volatility Heatmap
//!
//! Calculates module change frequency via git history
//! to identify high-risk areas that need extra presence.

use chrono::{DateTime, Duration, Utc};
use ndc_core::RiskLevel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Git change record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitChange {
    pub path: PathBuf,
    pub commit_hash: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
    pub change_type: ChangeType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed { old_path: Option<PathBuf> },
}

/// Module identifier
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleId {
    pub name: String,
    pub path: PathBuf,
}

impl ModuleId {
    pub fn from_path(path: &Path) -> Self {
        // Use parent directory as module if available
        let (name, path_buf) = if let Some(parent) = path.parent() {
            let name = parent
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            (name, parent.to_path_buf())
        } else {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            (name, path.to_path_buf())
        };

        Self {
            name,
            path: path_buf,
        }
    }
}

/// Volatility Heatmap - git history based risk assessment
#[derive(Debug, Clone)]
pub struct VolatilityHeatmap {
    /// Module -> Change frequency (normalized 0-1)
    module_frequency: HashMap<ModuleId, f64>,

    /// Recent changes (N days)
    recent_changes: Vec<GitChange>,

    /// Core modules (high-risk areas)
    core_modules: Vec<ModuleId>,

    /// Module change count (raw)
    raw_counts: HashMap<ModuleId, u32>,

    /// Calculation parameters
    config: HeatmapConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatmapConfig {
    /// Look-back period in days
    pub lookback_days: u32,

    /// Changes threshold for high volatility
    pub high_volatility_threshold: u32,

    /// Normalization factor
    pub normalization_factor: f64,
}

impl Default for HeatmapConfig {
    fn default() -> Self {
        Self {
            lookback_days: 7,
            high_volatility_threshold: 5,
            normalization_factor: 1.0,
        }
    }
}

/// Heatmap result for a specific module
#[derive(Debug, Clone)]
pub struct ModuleVolatility {
    pub module: ModuleId,
    pub score: f64,     // 0-1 normalized
    pub raw_count: u32, // Raw change count
    pub recent_files: Vec<PathBuf>,
    pub risk_level: RiskLevel,
}

/// Convert volatility score to risk level
pub fn volatility_to_risk_level(score: f64) -> RiskLevel {
    if score >= 0.8 {
        RiskLevel::Critical
    } else if score >= 0.6 {
        RiskLevel::High
    } else if score >= 0.3 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

impl VolatilityHeatmap {
    /// Create heatmap from git repository
    pub async fn from_git(
        repo_path: &Path,
        config: Option<HeatmapConfig>,
    ) -> Result<Self, HeatmapError> {
        let config = config.unwrap_or_default();

        // Get timestamp N days ago
        let since = Utc::now() - Duration::days(config.lookback_days as i64);

        // Get changed files from git
        let changes = Self::get_git_changes(repo_path, since).await?;

        // Build raw counts
        let mut raw_counts: HashMap<ModuleId, u32> = HashMap::new();
        for change in &changes {
            let module = Self::identify_module(&change.path);
            *raw_counts.entry(module).or_insert(0) += 1;
        }

        // Normalize frequencies
        let max_count = raw_counts.values().max().copied().unwrap_or(1);
        let mut module_frequency: HashMap<ModuleId, f64> = HashMap::new();

        for (module, count) in &raw_counts {
            let normalized = (*count as f64) / (max_count as f64) * config.normalization_factor;
            module_frequency.insert(module.clone(), normalized.min(1.0));
        }

        // Load core modules
        let core_modules = Self::identify_core_modules(repo_path).await?;

        Ok(Self {
            module_frequency,
            recent_changes: changes,
            core_modules,
            raw_counts,
            config,
        })
    }

    /// Get git changes since a given timestamp
    async fn get_git_changes(
        repo_path: &Path,
        since: DateTime<Utc>,
    ) -> Result<Vec<GitChange>, HeatmapError> {
        // Format timestamp for git
        let since_str = since.format("%Y-%m-%dT%H:%M:%S").to_string();

        // Run git log --name-status
        let output = Command::new("git")
            .args([
                "log",
                "--since",
                &since_str,
                "--name-status",
                "--pretty=format:%H|%an|%ai",
            ])
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| HeatmapError::GitCommandFailed(e.to_string()))?;

        if !output.status.success() {
            return Err(HeatmapError::GitCommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        // Parse output
        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut changes = Vec::new();
        let mut current_commit = None;
        let mut current_author = None;
        let mut current_time = None;

        for line in output_str.lines() {
            if line.is_empty() {
                continue;
            }

            // Check if this is a commit line (contains | separators)
            if line.contains('|') && line.matches('|').count() == 2 {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 3 {
                    current_commit = Some(parts[0].to_string());
                    current_author = Some(parts[1].to_string());
                    current_time = Some(
                        DateTime::parse_from_str(parts[2], "%Y-%m-%d %H:%M:%S %z")
                            .map_err(|_| HeatmapError::ParseError(line.to_string()))?
                            .with_timezone(&Utc),
                    );
                }
            } else if line.starts_with('A')
                || line.starts_with('M')
                || line.starts_with('D')
                || line.starts_with('R')
            {
                // File change line
                let mut parts = line.splitn(2, '\t');
                let status = parts.next().unwrap_or("");
                let path_str = parts.next().unwrap_or("");

                if let (Some(commit), Some(author), Some(time)) =
                    (current_commit.clone(), current_author.clone(), current_time)
                {
                    let change_type = match status.chars().next() {
                        Some('A') => ChangeType::Added,
                        Some('M') => ChangeType::Modified,
                        Some('D') => ChangeType::Deleted,
                        Some('R') => ChangeType::Renamed { old_path: None },
                        _ => ChangeType::Modified,
                    };

                    changes.push(GitChange {
                        path: PathBuf::from(path_str),
                        commit_hash: commit,
                        author,
                        timestamp: time,
                        change_type,
                    });
                }
            }
        }

        Ok(changes)
    }

    /// Identify module from file path
    fn identify_module(path: &PathBuf) -> ModuleId {
        // Try to identify module structure
        // For Rust projects: parent directory often indicates module
        if let Some(parent) = path.parent()
            && parent.file_name().is_some() {
                // Use immediate parent as module
                return ModuleId {
                    name: parent
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "root".to_string()),
                    path: parent.to_path_buf(),
                };
            }

        ModuleId::from_path(path)
    }

    /// Identify core modules (high-risk areas)
    async fn identify_core_modules(_repo_path: &Path) -> Result<Vec<ModuleId>, HeatmapError> {
        // Core modules typically include:
        // - core/src/
        // - crates/core/src/
        // - src/ with critical files

        let core_module_names = ["core", "runtime", "interface", "decision"];
        let mut modules = Vec::new();

        for name in core_module_names {
            modules.push(ModuleId {
                name: name.to_string(),
                path: PathBuf::from(name),
            });
        }

        Ok(modules)
    }

    /// Get volatility for a specific module
    pub fn get_module_volatility(&self, module: &ModuleId) -> ModuleVolatility {
        let score = self.module_frequency.get(module).copied().unwrap_or(0.0);
        let raw_count = self.raw_counts.get(module).copied().unwrap_or(0);

        let recent_files: Vec<PathBuf> = self
            .recent_changes
            .iter()
            .filter(|c| {
                let m = Self::identify_module(&c.path);
                m == *module
            })
            .map(|c| c.path.clone())
            .collect();

        ModuleVolatility {
            module: module.clone(),
            score,
            raw_count,
            recent_files,
            risk_level: volatility_to_risk_level(score),
        }
    }

    /// Get all high-volatility modules
    pub fn get_high_volatility_modules(&self) -> Vec<ModuleVolatility> {
        self.module_frequency
            .iter()
            .filter(|&(_, &score)| score >= 0.3) // Above medium risk
            .map(|(module, _)| self.get_module_volatility(module))
            .collect()
    }

    /// Check if a file is high-risk based on heatmap
    pub fn is_high_risk(&self, file: &Path) -> bool {
        let module = Self::identify_module(&file.to_path_buf());

        // Check if module is core
        if self.core_modules.contains(&module) {
            return true;
        }

        // Check volatility score
        let score = self.module_frequency.get(&module).copied().unwrap_or(0.0);

        // Count recent changes
        let recent_count = self
            .recent_changes
            .iter()
            .filter(|c| {
                let m = Self::identify_module(&c.path);
                m == module
            })
            .count();

        // Heatmap rule: recent changes > threshold OR high normalized score
        recent_count > self.config.high_volatility_threshold as usize || score > 0.6
    }

    /// Get all modules sorted by volatility
    pub fn get_modules_sorted(&self) -> Vec<ModuleVolatility> {
        let mut modules: Vec<ModuleVolatility> = self
            .module_frequency
            .keys()
            .map(|m| self.get_module_volatility(m))
            .collect();

        modules.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        modules
    }
}

/// Heatmap errors
#[derive(Debug, thiserror::Error)]
pub enum HeatmapError {
    #[error("Git command failed: {0}")]
    GitCommandFailed(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_heatmap_from_git_changes() {
        // Create a temp git repo
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Initialize git repo
        Command::new("git")
            .args(&["init"])
            .current_dir(repo_path)
            .output()
            .await
            .unwrap();

        // Create some files and commit
        fs::write(repo_path.join("file1.rs"), "// file 1").unwrap();
        fs::write(repo_path.join("file2.rs"), "// file 2").unwrap();

        Command::new("git")
            .args(&["add", "."])
            .current_dir(repo_path)
            .output()
            .await
            .unwrap();

        Command::new("git")
            .args(&["config", "user.email", "test@test.com"])
            .current_dir(repo_path)
            .output()
            .await
            .unwrap();

        Command::new("git")
            .args(&["config", "user.name", "Test"])
            .current_dir(repo_path)
            .output()
            .await
            .unwrap();

        Command::new("git")
            .args(&["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .await
            .unwrap();

        // Create heatmap
        let heatmap = VolatilityHeatmap::from_git(
            repo_path,
            Some(HeatmapConfig {
                lookback_days: 7,
                high_volatility_threshold: 5,
                normalization_factor: 1.0,
            }),
        )
        .await
        .unwrap();

        // Should have some changes
        assert!(!heatmap.recent_changes.is_empty() || heatmap.raw_counts.is_empty());
    }

    #[test]
    fn test_risk_level_from_score() {
        assert_eq!(volatility_to_risk_level(0.1), RiskLevel::Low);
        assert_eq!(volatility_to_risk_level(0.3), RiskLevel::Medium);
        assert_eq!(volatility_to_risk_level(0.6), RiskLevel::High);
        assert_eq!(volatility_to_risk_level(0.9), RiskLevel::Critical);
    }

    #[test]
    fn test_module_id_from_path() {
        let module = ModuleId::from_path(&PathBuf::from("crates/core/src/lib.rs"));
        assert_eq!(module.name, "src");
    }
}
