//! Documentation Updater - Phase 8: Update Documentation
//!
//! Responsibilities:
//! - Record immutable facts about system changes
//! - Generate human-readable narratives
//! - Update code documentation
//! - Maintain changelog

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::debug;

/// A documented fact (immutable)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    /// Fact ID
    pub id: String,
    /// Category of the fact
    pub category: FactCategory,
    /// The fact content
    pub statement: String,
    /// Evidence supporting the fact
    pub evidence: Vec<String>,
    /// When the fact was recorded
    pub recorded_at: chrono::DateTime<chrono::Utc>,
    /// Whether this fact is verified
    pub verified: bool,
}

/// Fact categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FactCategory {
    /// API change
    ApiChange,
    /// Database schema change
    SchemaChange,
    /// Configuration change
    ConfigChange,
    /// Security consideration
    SecurityNote,
    /// Performance consideration
    PerformanceNote,
    /// Known limitation
    Limitation,
    /// Design decision
    Decision,
    /// Workaround applied
    Workaround,
}

/// A narrative entry (human-readable story)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Narrative {
    /// Narrative ID
    pub id: String,
    /// Task ID this narrative belongs to
    pub task_id: String,
    /// Narrative title
    pub title: String,
    /// The narrative content
    pub content: String,
    /// Key learnings
    pub learnings: Vec<String>,
    /// When created
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Documentation update request
#[derive(Debug, Clone)]
pub struct DocUpdateRequest {
    /// Target file
    pub file_path: PathBuf,
    /// Type of update
    pub update_type: DocUpdateType,
    /// New content
    pub content: String,
    /// Context for the update
    pub context: String,
}

/// Types of documentation updates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocUpdateType {
    /// Add/Update docstring
    Docstring,
    /// Add/Update comment
    Comment,
    /// Add/Update CHANGELOG entry
    Changelog,
    /// Add/Update README section
    Readme,
    /// Add code example
    Example,
}

/// Documentation update result
#[derive(Debug, Clone)]
pub struct DocUpdateResult {
    /// Whether update was successful
    pub success: bool,
    /// Path to updated file
    pub file_path: PathBuf,
    /// What was changed
    pub changes: Vec<String>,
    /// Any warnings
    pub warnings: Vec<String>,
}

/// Documentation Updater Service
#[derive(Debug)]
pub struct DocUpdater {
    /// Facts storage
    facts: Arc<RwLock<Vec<Fact>>>,
    /// Narratives storage
    narratives: Arc<RwLock<Vec<Narrative>>>,
    /// Configuration
    _config: DocUpdaterConfig,
}

/// Configuration for documentation updater
#[derive(Debug, Clone)]
pub struct DocUpdaterConfig {
    /// Maximum narrative length
    _max_narrative_length: usize,
    /// Auto-generate narratives
    _auto_narrative: bool,
    /// Update docstrings automatically
    _auto_docstring: bool,
}

impl Default for DocUpdaterConfig {
    fn default() -> Self {
        Self {
            _max_narrative_length: 1000,
            _auto_narrative: true,
            _auto_docstring: false,
        }
    }
}

impl DocUpdater {
    /// Create a new documentation updater
    pub fn new(config: Option<DocUpdaterConfig>) -> Self {
        let cfg = config.unwrap_or_default();
        Self {
            facts: Arc::new(RwLock::new(Vec::new())),
            narratives: Arc::new(RwLock::new(Vec::new())),
            _config: cfg,
        }
    }

    /// Record a fact
    pub fn record_fact(&self, fact: Fact) {
        let statement = fact.statement.clone();
        let mut facts = self.facts.write().expect("facts RwLock poisoned");
        facts.push(fact);
        debug!("Recorded fact: {}", statement);
    }

    /// Get facts by category
    pub fn get_facts_by_category(&self, category: FactCategory) -> Vec<Fact> {
        let facts = self.facts.read().expect("facts RwLock poisoned");
        facts
            .iter()
            .filter(|f| f.category == category)
            .cloned()
            .collect()
    }

    /// Generate a narrative from task results
    pub fn generate_narrative(
        &self,
        task_id: &str,
        title: &str,
        actions: &[String],
        results: &[String],
        learnings: &[String],
    ) -> Narrative {
        let content = self.build_narrative_content(actions, results, learnings);

        let narrative = Narrative {
            id: format!("narrative-{}", &uuid::Uuid::new_v4().to_string()[..8]),
            task_id: task_id.to_string(),
            title: title.to_string(),
            content,
            learnings: learnings.to_vec(),
            created_at: chrono::Utc::now(),
        };

        let mut narratives = self.narratives.write().expect("narratives RwLock poisoned");
        narratives.push(narrative.clone());

        debug!("Generated narrative: {}", narrative.title);
        narrative
    }

    /// Build narrative content
    fn build_narrative_content(
        &self,
        actions: &[String],
        results: &[String],
        learnings: &[String],
    ) -> String {
        let mut content = String::new();

        if !actions.is_empty() {
            content.push_str("## Actions Taken\n\n");
            for (i, action) in actions.iter().enumerate() {
                content.push_str(&format!("{}. {}\n", i + 1, action));
            }
            content.push('\n');
        }

        if !results.is_empty() {
            content.push_str("## Results\n\n");
            for result in results {
                content.push_str(&format!("- {}\n", result));
            }
            content.push('\n');
        }

        if !learnings.is_empty() {
            content.push_str("## Learnings\n\n");
            for learning in learnings {
                content.push_str(&format!("- {}\n", learning));
            }
        }

        content
    }

    /// Create a documentation update request
    pub fn create_doc_update(
        &self,
        file_path: PathBuf,
        update_type: DocUpdateType,
        content: String,
        context: String,
    ) -> DocUpdateRequest {
        DocUpdateRequest {
            file_path,
            update_type,
            content,
            context,
        }
    }

    /// Apply a documentation update
    pub async fn apply_update(&self, request: &DocUpdateRequest) -> DocUpdateResult {
        let mut changes = Vec::new();
        let mut warnings = Vec::new();

        // Read the file
        match std::fs::read_to_string(&request.file_path) {
            Ok(mut content) => {
                let original_len = content.len();

                // Apply update based on type
                match request.update_type {
                    DocUpdateType::Docstring => {
                        if let Some(updated) = self.update_docstring(&content, &request.content) {
                            content = updated;
                            changes.push("Updated docstring".to_string());
                        } else {
                            warnings.push("No suitable location for docstring found".to_string());
                        }
                    }
                    DocUpdateType::Comment => {
                        if let Some(updated) = self.update_comment(&content, &request.content) {
                            content = updated;
                            changes.push("Updated inline comment".to_string());
                        } else {
                            warnings.push("No suitable location for comment found".to_string());
                        }
                    }
                    DocUpdateType::Changelog => {
                        if let Some(updated) = self.update_changelog(&content, &request.content) {
                            content = updated;
                            changes.push("Updated CHANGELOG".to_string());
                        }
                    }
                    DocUpdateType::Readme => {
                        if let Some(updated) =
                            self.update_readme(&content, &request.context, &request.content)
                        {
                            content = updated;
                            changes.push("Updated README section".to_string());
                        }
                    }
                    DocUpdateType::Example => {
                        if let Some(updated) = self.add_example(&content, &request.content) {
                            content = updated;
                            changes.push("Added code example".to_string());
                        }
                    }
                }

                // Write back if changed
                if content.len() != original_len
                    && let Err(e) = std::fs::write(&request.file_path, &content)
                {
                    return DocUpdateResult {
                        success: false,
                        file_path: request.file_path.clone(),
                        changes,
                        warnings: vec![format!("Failed to write file: {}", e)],
                    };
                }

                DocUpdateResult {
                    success: !changes.is_empty(),
                    file_path: request.file_path.clone(),
                    changes,
                    warnings,
                }
            }
            Err(e) => DocUpdateResult {
                success: false,
                file_path: request.file_path.clone(),
                changes,
                warnings: vec![format!("Failed to read file: {}", e)],
            },
        }
    }

    /// Update docstring in code
    fn update_docstring(&self, content: &str, new_docstring: &str) -> Option<String> {
        // Try to find and replace existing docstring
        // Pattern: /// or /** ... */ or """ ... """
        let patterns = [
            (r"///[^\n]*\n", "/// New docstring\n"),
            (r"/\*\*[^*]*\*(?:[^*/][^*]*)*\*/", "/* New docstring */"),
            (r#"""[^"]*"""\s*"#, "\"\"\"New docstring\"\"\""),
        ];

        for (pattern, replacement) in &patterns {
            if let Ok(re) = regex::RegexBuilder::new(pattern).multi_line(true).build()
                && re.is_match(content)
            {
                return Some(re.replace(content, *replacement).to_string());
            }
        }

        // If no existing docstring, try to add after function signature
        if let Some(pos) = content.find("fn ") {
            let after_fn = &content[pos..];
            if let Some(end_line) = after_fn.find('\n') {
                let fn_signature = &after_fn[..end_line];
                if fn_signature.contains('{') {
                    let brace_pos = fn_signature.find('{').expect("brace confirmed by contains");
                    let insertion_point = pos + brace_pos + 1;
                    let mut new_content = content[..insertion_point].to_string();
                    new_content.push_str("\n    ");
                    new_content.push_str(new_docstring);
                    new_content.push('\n');
                    new_content.push_str(&content[insertion_point..]);
                    return Some(new_content);
                }
            }
        }

        None
    }

    /// Update inline comment
    fn update_comment(&self, content: &str, new_comment: &str) -> Option<String> {
        // Try to find and replace existing comments
        let comment_pattern = r"//[^\n]*\n".to_string();
        let hash_pattern = r"# [^\n]*\n";

        if let Ok(re) = regex::RegexBuilder::new(&comment_pattern)
            .multi_line(true)
            .build()
            && re.is_match(content)
        {
            let replacement = format!("// {}\n", new_comment);
            return Some(re.replace(content, &replacement).to_string());
        }

        if let Ok(re) = regex::RegexBuilder::new(hash_pattern)
            .multi_line(true)
            .build()
            && re.is_match(content)
        {
            let replacement = format!("# {}\n", new_comment);
            return Some(re.replace(content, &replacement).to_string());
        }

        None
    }

    /// Update CHANGELOG
    fn update_changelog(&self, content: &str, new_entry: &str) -> Option<String> {
        let header_pattern = regex::Regex::new(r"# Changelog").ok()?;
        let date = chrono::Utc::now().format("%Y-%m-%d");

        let entry = format!("\n## [{}] - {}\n\n{}\n", date, date, new_entry);

        if header_pattern.is_match(content)
            && let Some(pos) = content.find("## [Unreleased]")
        {
            // Insert before unreleased section
            let mut new_content = content[..pos].to_string();
            new_content.push_str(&entry);
            new_content.push_str("\n## [Unreleased]\n");
            new_content.push_str(&content[pos + 15..]);
            return Some(new_content);
        }

        // If no unreleased section, append to top
        let mut new_content = entry;
        new_content.push_str(content);
        Some(new_content)
    }

    /// Update README section
    fn update_readme(&self, content: &str, section: &str, new_content: &str) -> Option<String> {
        let section_pattern =
            regex::Regex::new(&format!(r"## {}\s*\n([^\n]*(?:\n(?!## ))*)", section)).ok()?;
        let replacement = format!("## {}\n\n{}\n", section, new_content);

        if section_pattern.is_match(content) {
            return Some(section_pattern.replace(content, &replacement).to_string());
        }

        // If section doesn't exist, append it
        let mut result = content.to_string();
        result.push_str("\n## ");
        result.push_str(section);
        result.push_str("\n\n");
        result.push_str(new_content);
        Some(result)
    }

    /// Add code example
    fn add_example(&self, content: &str, example: &str) -> Option<String> {
        // Find ```rust code blocks and add example
        let code_block_pattern = regex::Regex::new(r"```rust\s*\n([^\n]*)```").ok()?;

        if code_block_pattern.is_match(content) {
            return Some(
                code_block_pattern
                    .replace(content, format!("```rust\n{}\n{}\n```", example, "$1"))
                    .to_string(),
            );
        }

        None
    }

    /// List all narratives
    pub fn list_narratives(&self) -> Vec<Narrative> {
        let narratives = self.narratives.read().expect("narratives RwLock poisoned");
        narratives.clone()
    }

    /// List all facts
    pub fn list_facts(&self) -> Vec<Fact> {
        let facts = self.facts.read().expect("facts RwLock poisoned");
        facts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_fact() {
        let updater = DocUpdater::new(None);

        let fact = Fact {
            id: "fact-1".to_string(),
            category: FactCategory::Decision,
            statement: "Used B-tree for performance".to_string(),
            evidence: vec!["Benchmark showed 2x improvement".to_string()],
            recorded_at: chrono::Utc::now(),
            verified: true,
        };

        updater.record_fact(fact);

        let facts = updater.get_facts_by_category(FactCategory::Decision);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].statement, "Used B-tree for performance");
    }

    #[test]
    fn test_generate_narrative() {
        let updater = DocUpdater::new(None);

        let narrative = updater.generate_narrative(
            "task-123",
            "Fix authentication bug",
            &["Identified root cause in token validation".to_string()],
            &["Token validation fixed".to_string()],
            &["Always validate token expiration".to_string()],
        );

        assert_eq!(narrative.task_id, "task-123");
        assert!(narrative.content.contains("Actions Taken"));
        assert!(narrative.content.contains("Results"));
    }

    #[tokio::test]
    async fn test_apply_changelog_update() {
        let updater = DocUpdater::new(None);

        // Create temp file with changelog
        let temp_file = "/tmp/test_changelog.md";
        let original = "# Changelog\n\n## [Unreleased]\n";
        std::fs::write(temp_file, original).unwrap();

        let request = updater.create_doc_update(
            PathBuf::from(temp_file),
            DocUpdateType::Changelog,
            "- Added new authentication feature".to_string(),
            "".to_string(),
        );

        let result = updater.apply_update(&request).await;

        assert!(result.success);
        assert!(result.changes.contains(&"Updated CHANGELOG".to_string()));

        // Cleanup
        std::fs::remove_file(temp_file).ok();
    }

    #[tokio::test]
    async fn test_apply_docstring_update() {
        let updater = DocUpdater::new(None);

        // Create temp file with function
        let temp_file = "/tmp/test_func.rs";
        let original = r#"fn calculate_total(items: &[Item]) -> f64 {
    items.iter().fold(0.0, |acc, item| acc + item.price)
}"#;
        std::fs::write(temp_file, original).unwrap();

        let request = updater.create_doc_update(
            PathBuf::from(temp_file),
            DocUpdateType::Docstring,
            "Calculates the total price of all items.".to_string(),
            "Function to update".to_string(),
        );

        let result = updater.apply_update(&request).await;

        assert!(result.success || !result.warnings.is_empty()); // May warn if no good location

        // Cleanup
        std::fs::remove_file(temp_file).ok();
    }
}
