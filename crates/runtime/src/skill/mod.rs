//! Skills System - Markdown-based skill definitions
//!
//! Responsibilities:
//! - Skill discovery and loading from filesystem
//! - Skill metadata parsing (YAML frontmatter)
//! - Template variable substitution
//! - Skill execution engine
//! - Integration with LLM and tool system

use glob::glob;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

pub mod executor;
pub use executor::{SkillExecutionContext, SkillExecutor, SkillResult};

/// Skill definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub parameters: Vec<SkillParameter>,
    #[serde(default)]
    pub examples: Vec<SkillExample>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillParameter {
    pub name: String,
    pub r#type: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExample {
    pub description: String,
    pub invocation: String,
}

/// Skill registry
#[derive(Debug, Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
    categories: HashMap<String, Vec<String>>,
    tags_index: HashMap<String, Vec<String>>,
    discovery_paths: Vec<PathBuf>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            categories: HashMap::new(),
            tags_index: HashMap::new(),
            discovery_paths: Vec::new(),
        }
    }

    pub fn add_discovery_path(&mut self, path: PathBuf) {
        self.discovery_paths.push(path);
    }

    pub fn set_default_paths(&mut self) {
        let default_paths = [".claude/skills/", ".opencode/skills/", ".agents/"];

        for dir in &default_paths {
            self.add_discovery_path(PathBuf::from(dir));
        }
    }

    pub async fn discover_and_load(&mut self) -> Result<usize, String> {
        let mut loaded = 0;

        // Clone paths to avoid borrow issues
        let paths: Vec<PathBuf> = self.discovery_paths.clone();

        for base_path in paths {
            let pattern = format!("{}/**/SKILL.md", base_path.display());

            for entry in glob(&pattern).map_err(|e| format!("Glob error: {}", e))? {
                match entry {
                    Ok(file_path) => {
                        if let Some(1) = self.load_skill(&file_path).await? {
                            loaded += 1;
                        }
                    }
                    Err(e) => {
                        warn!("Glob entry error: {}", e);
                    }
                }
            }
        }

        info!("Loaded {} skills", loaded);
        Ok(loaded)
    }

    pub async fn load_skill(&mut self, path: &PathBuf) -> Result<Option<usize>, String> {
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path).map_err(|e| format!("Read failed: {}", e))?;

        let skill = Self::parse_skill(&content, path)?;

        if self.skills.contains_key(&skill.name) {
            warn!("Skill {} already exists, skipping", skill.name);
            return Ok(None);
        }

        self.skills.insert(skill.name.clone(), skill.clone());

        if let Some(ref cat) = skill.category {
            self.categories
                .entry(cat.clone())
                .or_default()
                .push(skill.name.clone());
        }

        for tag in &skill.tags {
            self.tags_index
                .entry(tag.clone())
                .or_default()
                .push(skill.name.clone());
        }

        debug!("Loaded skill: {}", skill.name);
        Ok(Some(1))
    }

    fn parse_skill(content: &str, _path: &PathBuf) -> Result<Skill, String> {
        let (frontmatter, body) = Self::extract_frontmatter(content)?;

        let metadata: SkillMetadata = if !frontmatter.is_empty() {
            serde_yaml::from_str(&frontmatter).map_err(|e| format!("YAML parse failed: {}", e))?
        } else {
            SkillMetadata::default()
        };

        let examples = Self::parse_examples(&body);

        Ok(Skill {
            name: metadata.name.unwrap_or_else(|| "unknown".to_string()),
            description: metadata.description.unwrap_or_default(),
            category: metadata.category,
            tags: metadata.tags.unwrap_or_default(),
            parameters: metadata.parameters.unwrap_or_default(),
            examples,
            content: body.to_string(),
        })
    }

    fn extract_frontmatter(content: &str) -> Result<(String, String), String> {
        if !content.starts_with("---") {
            return Ok((String::new(), content.to_string()));
        }

        let mut lines = content.lines();
        lines.next();

        let mut frontmatter = String::new();
        let mut body_start = None;

        for (i, line) in lines.enumerate() {
            if line == "---" {
                body_start = Some(i + 1);
                break;
            }
            frontmatter.push_str(line);
            frontmatter.push('\n');
        }

        let start = body_start.ok_or_else(|| "Unclosed frontmatter".to_string())?;

        let body = content
            .lines()
            .skip(start)
            .collect::<Vec<&str>>()
            .join("\n");

        Ok((frontmatter, body))
    }

    fn parse_examples(body: &str) -> Vec<SkillExample> {
        let mut examples = Vec::new();
        let lines: Vec<&str> = body.lines().collect();

        for i in 0..lines.len() {
            if lines[i].trim_start().starts_with("```") {
                let mut desc = String::new();
                let mut j = i;

                while j > 0 {
                    let line = lines[j - 1].trim();
                    if !line.is_empty() && !line.starts_with('#') && !line.starts_with("```") {
                        desc = line.to_string();
                        break;
                    }
                    j -= 1;
                }

                let mut k = i + 1;
                let mut code = String::new();
                while k < lines.len() {
                    if lines[k].trim_start().starts_with("```") {
                        break;
                    }
                    code.push_str(lines[k]);
                    code.push('\n');
                    k += 1;
                }

                if !code.trim().is_empty() {
                    examples.push(SkillExample {
                        description: desc,
                        invocation: code.trim().to_string(),
                    });
                }
            }
        }

        examples
    }

    pub fn get_all(&self) -> &HashMap<String, Skill> {
        &self.skills
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    pub fn get_by_category(&self, category: &str) -> Vec<&Skill> {
        if let Some(names) = self.categories.get(category) {
            names.iter().filter_map(|n| self.skills.get(n)).collect()
        } else {
            Vec::new()
        }
    }

    pub fn get_by_tag(&self, tag: &str) -> Vec<&Skill> {
        if let Some(names) = self.tags_index.get(tag) {
            names.iter().filter_map(|n| self.skills.get(n)).collect()
        } else {
            Vec::new()
        }
    }

    pub fn search(&self, query: &str) -> Vec<&Skill> {
        let query = query.to_lowercase();
        self.skills
            .values()
            .filter(|skill| {
                skill.name.to_lowercase().contains(&query)
                    || skill.description.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub fn get_categories(&self) -> Vec<String> {
        self.categories.keys().cloned().collect()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        registry.set_default_paths();
        registry
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct SkillMetadata {
    name: Option<String>,
    description: Option<String>,
    category: Option<String>,
    tags: Option<Vec<String>>,
    parameters: Option<Vec<SkillParameter>>,
}
