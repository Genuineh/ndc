//! Knowledge Understanding Service - Phase 1: Requirement Analysis
//!
//! Responsibilities:
//! - Parse and analyze user requirements
//! - Retrieve relevant knowledge from knowledge base
//! - Extract entities and relationships
//! - Build understanding context

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// User requirement for understanding
#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    /// Original request text
    pub text: String,
    /// Extracted intent
    pub intent: RequirementIntent,
    /// Identified entities
    pub entities: Vec<Entity>,
    /// Identified relationships
    pub relationships: Vec<Relationship>,
    /// Extracted constraints
    pub constraints: Vec<Constraint>,
    /// Quality indicators
    pub quality: RequirementQuality,
}

/// Requirement intent types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequirementIntent {
    CreateFeature,
    ModifyFeature,
    FixBug,
    RemoveFeature,
    Refactor,
    Investigate,
    Optimize,
    Document,
    Test,
    Unknown,
}

/// An entity extracted from requirements
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    /// Entity name
    pub name: String,
    /// Entity type (file, function, module, api, etc.)
    pub entity_type: EntityType,
    /// Entity location (if applicable)
    pub location: Option<PathBuf>,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
}

/// Entity types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    File,
    Function,
    Module,
    Api,
    Database,
    Config,
    Test,
    Documentation,
    ExternalDependency,
    Unknown,
}

/// A relationship between entities
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relationship {
    /// Source entity
    pub source: String,
    /// Target entity
    pub target: String,
    /// Relationship type
    pub relation_type: RelationType,
    /// Confidence score
    pub confidence: f32,
}

/// Relationship types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationType {
    DependsOn,
    Implements,
    Calls,
    Inherits,
    Configures,
    Tests,
    Documents,
    Unknown,
}

/// A constraint extracted from requirements
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraint {
    /// Constraint description
    pub description: String,
    /// Constraint type
    pub constraint_type: ConstraintType,
    /// Priority (1 = critical, 5 = low)
    pub priority: u8,
    /// Whether this is a hard constraint
    pub is_hard: bool,
}

/// Constraint types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConstraintType {
    Performance,
    Security,
    Compatibility,
    Scalability,
    Reliability,
    Usability,
    Cost,
    Time,
    Regulatory,
    Other,
}

/// Quality assessment of a requirement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequirementQuality {
    /// Clarity score (0.0 - 1.0)
    pub clarity: f32,
    /// Completeness score (0.0 - 1.0)
    pub completeness: f32,
    /// Consistency score (0.0 - 1.0)
    pub consistency: f32,
    /// Overall quality
    pub overall: f32,
    /// Missing information
    pub missing_info: Vec<String>,
}

/// Understanding context built from requirements
#[derive(Debug, Clone)]
pub struct UnderstandingContext {
    /// The original requirement
    pub requirement: Requirement,
    /// Retrieved relevant knowledge
    pub relevant_knowledge: Vec<KnowledgeItem>,
    /// Identified gaps in knowledge
    pub knowledge_gaps: Vec<String>,
    /// Suggested actions
    pub suggested_actions: Vec<String>,
    /// Confidence in understanding
    pub confidence: f32,
}

/// A knowledge item from the knowledge base
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeItem {
    /// Knowledge ID
    pub id: String,
    /// Knowledge title
    pub title: String,
    /// Knowledge summary
    pub summary: String,
    /// Knowledge type
    pub knowledge_type: KnowledgeType,
    /// Related files
    pub related_files: Vec<PathBuf>,
    /// Relevance score (0.0 - 1.0)
    pub relevance_score: f32,
}

/// Knowledge types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KnowledgeType {
    CodeDocumentation,
    ApiDocumentation,
    DesignDocument,
    DecisionRecord,
    ErrorSolution,
    TestResult,
    ProjectStructure,
    Configuration,
}

/// Knowledge Understanding Service
#[derive(Debug)]
pub struct KnowledgeUnderstandingService {
    /// Knowledge storage
    knowledge: Arc<RwLock<HashMap<String, KnowledgeItem>>>,
    /// Configuration
    config: UnderstandingConfig,
}

/// Configuration for understanding service
#[derive(Debug, Clone)]
pub struct UnderstandingConfig {
    /// Minimum confidence threshold
    #[allow(dead_code)]
    _min_confidence: f32,
    /// Maximum knowledge items to retrieve
    max_knowledge_items: usize,
    /// Entity extraction patterns
    #[allow(dead_code)]
    _entity_patterns: HashMap<EntityType, Vec<String>>,
}

impl Default for UnderstandingConfig {
    fn default() -> Self {
        let mut patterns = HashMap::new();
        patterns.insert(EntityType::File, vec![".rs".to_string(), ".py".to_string(), ".js".to_string(), ".ts".to_string(), ".json".to_string(), ".yaml".to_string()]);
        patterns.insert(EntityType::Function, vec!["fn ".to_string(), "function ".to_string(), "def ".to_string(), "class ".to_string()]);
        patterns.insert(EntityType::Api, vec!["endpoint".to_string(), "api".to_string(), "route".to_string(), "handler".to_string()]);
        patterns.insert(EntityType::Database, vec!["table".to_string(), "query".to_string(), "schema".to_string(), "database".to_string()]);
        patterns.insert(EntityType::Config, vec!["config".to_string(), "setting".to_string(), "configuration".to_string()]);

        Self {
            _min_confidence: 0.5,
            max_knowledge_items: 10,
            _entity_patterns: patterns,
        }
    }
}

impl KnowledgeUnderstandingService {
    /// Create a new understanding service
    pub fn new(config: Option<UnderstandingConfig>) -> Self {
        let cfg = config.unwrap_or_default();
        Self {
            knowledge: Arc::new(RwLock::new(HashMap::new())),
            config: cfg,
        }
    }

    /// Parse and understand a user requirement
    pub async fn understand_requirement(&self, requirement_text: &str) -> Requirement {
        let intent = self.extract_intent(requirement_text);
        let entities = self.extract_entities(requirement_text);
        let relationships = self.extract_relationships(&entities, requirement_text);
        let constraints = self.extract_constraints(requirement_text);
        let quality = self.assess_quality(requirement_text, &entities, &constraints);

        Requirement {
            text: requirement_text.to_string(),
            intent,
            entities,
            relationships,
            constraints,
            quality,
        }
    }

    /// Build understanding context from a requirement
    pub async fn build_context(&self, requirement: &Requirement) -> UnderstandingContext {
        // Retrieve relevant knowledge
        let relevant_knowledge = self.retrieve_knowledge(requirement).await;

        // Identify knowledge gaps
        let knowledge_gaps = self.identify_gaps(requirement, &relevant_knowledge);

        // Generate suggested actions
        let suggested_actions = self.generate_actions(requirement, &knowledge_gaps);

        // Calculate overall confidence
        let confidence = self.calculate_confidence(requirement, &relevant_knowledge);

        UnderstandingContext {
            requirement: requirement.clone(),
            relevant_knowledge,
            knowledge_gaps,
            suggested_actions,
            confidence,
        }
    }

    /// Extract intent from requirement text
    fn extract_intent(&self, text: &str) -> RequirementIntent {
        let text_lower = text.to_lowercase();

        if text_lower.contains("create") || text_lower.contains("add") || text_lower.contains("implement") {
            return RequirementIntent::CreateFeature;
        }
        if text_lower.contains("modify") || text_lower.contains("change") || text_lower.contains("update") {
            return RequirementIntent::ModifyFeature;
        }
        if text_lower.contains("fix") || text_lower.contains("bug") || text_lower.contains("error") {
            return RequirementIntent::FixBug;
        }
        if text_lower.contains("remove") || text_lower.contains("delete") || text_lower.contains("eliminate") {
            return RequirementIntent::RemoveFeature;
        }
        if text_lower.contains("refactor") || text_lower.contains("restructure") {
            return RequirementIntent::Refactor;
        }
        if text_lower.contains("investigate") || text_lower.contains("explore") || text_lower.contains("find") {
            return RequirementIntent::Investigate;
        }
        if text_lower.contains("optimize") || text_lower.contains("performance") || text_lower.contains("speed") {
            return RequirementIntent::Optimize;
        }
        if text_lower.contains("document") || text_lower.contains("docs") {
            return RequirementIntent::Document;
        }
        if text_lower.contains("test") || text_lower.contains("verify") {
            return RequirementIntent::Test;
        }

        RequirementIntent::Unknown
    }

    /// Extract entities from requirement text
    fn extract_entities(&self, text: &str) -> Vec<Entity> {
        let mut entities = Vec::new();
        let text_lower = text.to_lowercase();

        // Extract file entities (using simple string matching)
        let file_extensions = [".rs", ".py", ".js", ".ts", ".json", ".yaml", ".toml", ".md"];
        for ext in &file_extensions {
            let mut pos = 0usize;
            while let Some(ext_pos) = text[pos..].find(ext) {
                let absolute_pos = pos + ext_pos;
                // Find the start of the file name by looking backwards
                let word_start = text[..absolute_pos]
                    .rfind(|c: char| c == ' ' || c == '/' || c == '\\' || c == '"' || c == '\'')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let file_name = &text[word_start..absolute_pos + ext.len()];

                if !file_name.is_empty() && file_name.ends_with(ext) {
                    entities.push(Entity {
                        name: file_name.to_string(),
                        entity_type: EntityType::File,
                        location: Some(PathBuf::from(&file_name)),
                        confidence: 0.9,
                    });
                }
                pos = absolute_pos + 1; // Continue searching after this match
            }
        }

        // Extract API entities
        if text_lower.contains("api") || text_lower.contains("endpoint") || text_lower.contains("route") {
            entities.push(Entity {
                name: "API".to_string(),
                entity_type: EntityType::Api,
                location: None,
                confidence: 0.7,
            });
        }

        // Extract function/method mentions (simple pattern matching)
        let func_patterns = ["fn ", "function ", "def ", "method "];
        for pattern in &func_patterns {
            if text_lower.contains(pattern) {
                if let Some(pos) = text_lower.find(pattern) {
                    // Use original text for case-preserving extraction
                    let after = &text[pos + pattern.len()..];
                    let func_name: String = after
                        .chars()
                        .take_while(|c| *c == '_' || c.is_alphanumeric())
                        .collect();
                    if !func_name.is_empty() {
                        entities.push(Entity {
                            name: func_name.to_string(),
                            entity_type: EntityType::Function,
                            location: None,
                            confidence: 0.8,
                        });
                    }
                }
            }
        }

        // Extract database mentions
        if text_lower.contains("database") || text_lower.contains("table") || text_lower.contains("query") {
            entities.push(Entity {
                name: "Database".to_string(),
                entity_type: EntityType::Database,
                location: None,
                confidence: 0.7,
            });
        }

        // Extract configuration mentions
        if text_lower.contains("config") || text_lower.contains("setting") {
            entities.push(Entity {
                name: "Configuration".to_string(),
                entity_type: EntityType::Config,
                location: None,
                confidence: 0.7,
            });
        }

        entities
    }

    /// Extract relationships between entities
    fn extract_relationships(
        &self,
        entities: &[Entity],
        text: &str,
    ) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        let text_lower = text.to_lowercase();

        for entity in entities {
            // Check for calls relationship
            if text_lower.contains("calls") || text_lower.contains("uses") {
                if entity.entity_type == EntityType::Function {
                    relationships.push(Relationship {
                        source: entity.name.clone(),
                        target: "unknown".to_string(),
                        relation_type: RelationType::Calls,
                        confidence: 0.6,
                    });
                }
            }

            // Check for depends on relationship
            if text_lower.contains("depends") || text_lower.contains("requires") {
                relationships.push(Relationship {
                    source: entity.name.clone(),
                    target: "dependency".to_string(),
                    relation_type: RelationType::DependsOn,
                    confidence: 0.7,
                });
            }

            // Check for tests relationship
            if text_lower.contains("test") || text_lower.contains("spec") {
                relationships.push(Relationship {
                    source: entity.name.clone(),
                    target: "test".to_string(),
                    relation_type: RelationType::Tests,
                    confidence: 0.8,
                });
            }
        }

        relationships
    }

    /// Extract constraints from requirement text
    fn extract_constraints(&self, text: &str) -> Vec<Constraint> {
        let mut constraints = Vec::new();
        let text_lower = text.to_lowercase();

        // Performance constraints
        if text_lower.contains("fast") || text_lower.contains("performance") || text_lower.contains("speed") {
            constraints.push(Constraint {
                description: "Performance requirement".to_string(),
                constraint_type: ConstraintType::Performance,
                priority: 2,
                is_hard: text_lower.contains("must") || text_lower.contains("required"),
            });
        }

        // Security constraints
        if text_lower.contains("secure") || text_lower.contains("auth") || text_lower.contains("permission") {
            constraints.push(Constraint {
                description: "Security requirement".to_string(),
                constraint_type: ConstraintType::Security,
                priority: 1,
                is_hard: true,
            });
        }

        // Compatibility constraints
        if text_lower.contains("compatible") || text_lower.contains("support") {
            constraints.push(Constraint {
                description: "Compatibility requirement".to_string(),
                constraint_type: ConstraintType::Compatibility,
                priority: 3,
                is_hard: false,
            });
        }

        constraints
    }

    /// Assess requirement quality
    fn assess_quality(
        &self,
        text: &str,
        entities: &[Entity],
        constraints: &[Constraint],
    ) -> RequirementQuality {
        let clarity = if text.len() > 50 && text.len() < 1000 { 0.8 } else { 0.6 };
        let completeness = if !entities.is_empty() && !constraints.is_empty() { 0.8 } else { 0.5 };
        let consistency = 0.9; // Assume consistent unless we detect contradictions
        let overall = (clarity + completeness + consistency) / 3.0;

        let mut missing_info = Vec::new();
        if entities.is_empty() {
            missing_info.push("No specific entities identified".to_string());
        }
        if constraints.is_empty() {
            missing_info.push("No explicit constraints found".to_string());
        }

        RequirementQuality {
            clarity,
            completeness,
            consistency,
            overall,
            missing_info,
        }
    }

    /// Retrieve relevant knowledge for a requirement
    async fn retrieve_knowledge(&self, _requirement: &Requirement) -> Vec<KnowledgeItem> {
        let knowledge = self.knowledge.read().unwrap();
        let mut relevant: Vec<_> = knowledge.values().cloned().collect();

        // Score and sort by relevance
        relevant.sort_by(|a, b| {
            b.relevance_score.partial_cmp(&a.relevance_score).unwrap_or(std::cmp::Ordering::Equal)
        });

        relevant.truncate(self.config.max_knowledge_items);
        relevant
    }

    /// Identify gaps in knowledge
    fn identify_gaps(&self, requirement: &Requirement, knowledge: &[KnowledgeItem]) -> Vec<String> {
        let mut gaps = Vec::new();

        for entity in &requirement.entities {
            let found = knowledge.iter().any(|k| {
                k.related_files.iter().any(|f| {
                    f.to_string_lossy().contains(&entity.name)
                })
            });

            if !found && entity.location.is_some() {
                gaps.push(format!("No knowledge found for: {}", entity.name));
            }
        }

        gaps
    }

    /// Generate suggested actions based on understanding
    fn generate_actions(
        &self,
        requirement: &Requirement,
        gaps: &[String],
    ) -> Vec<String> {
        let mut actions = Vec::new();

        // Suggest documentation for undocumented entities
        for entity in &requirement.entities {
            if entity.confidence < 0.7 {
                actions.push(format!("Verify entity: {}", entity.name));
            }
        }

        // Suggest addressing knowledge gaps
        for gap in gaps {
            actions.push(format!("Research: {}", gap));
        }

        // Suggest based on intent
        match requirement.intent {
            RequirementIntent::CreateFeature => {
                actions.push("Create design document".to_string());
                actions.push("Write unit tests".to_string());
            }
            RequirementIntent::FixBug => {
                actions.push("Reproduce the bug".to_string());
                actions.push("Identify root cause".to_string());
                actions.push("Add regression test".to_string());
            }
            RequirementIntent::Refactor => {
                actions.push("Review affected code".to_string());
                actions.push("Update tests".to_string());
            }
            _ => {
                actions.push("Review requirements".to_string());
            }
        }

        actions
    }

    /// Calculate overall confidence in understanding
    fn calculate_confidence(&self, requirement: &Requirement, knowledge: &[KnowledgeItem]) -> f32 {
        let entity_confidence: f32 = requirement.entities.iter().map(|e| e.confidence).sum();
        let entity_score = if !requirement.entities.is_empty() {
            entity_confidence / requirement.entities.len() as f32
        } else {
            0.5
        };

        let knowledge_score = if !knowledge.is_empty() {
            knowledge.iter().map(|k| k.relevance_score).sum::<f32>() / knowledge.len() as f32
        } else {
            0.3
        };

        let quality_score = requirement.quality.overall;

        entity_score * 0.3 + knowledge_score * 0.3 + quality_score * 0.4
    }

    /// Add knowledge to the service
    pub fn add_knowledge(&self, knowledge: KnowledgeItem) {
        let mut store = self.knowledge.write().unwrap();
        store.insert(knowledge.id.clone(), knowledge);
    }

    /// Get knowledge by ID
    pub fn get_knowledge(&self, id: &str) -> Option<KnowledgeItem> {
        let store = self.knowledge.read().unwrap();
        store.get(id).cloned()
    }

    /// List all knowledge
    pub fn list_knowledge(&self) -> Vec<KnowledgeItem> {
        let store = self.knowledge.read().unwrap();
        store.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_understand_create_feature() {
        let service = KnowledgeUnderstandingService::new(None);

        let requirement = service
            .understand_requirement("Create a new API endpoint for user authentication in users.rs")
            .await;

        assert_eq!(requirement.intent, RequirementIntent::CreateFeature);
        assert!(!requirement.entities.is_empty());
        assert!(requirement.entities.iter().any(|e| e.name.contains("users.rs")));
    }

    #[tokio::test]
    async fn test_understand_fix_bug() {
        let service = KnowledgeUnderstandingService::new(None);

        let requirement = service
            .understand_requirement("Fix the performance bug in the database query function")
            .await;

        assert_eq!(requirement.intent, RequirementIntent::FixBug);
        assert!(requirement.constraints.iter().any(|c| c.constraint_type == ConstraintType::Performance));
    }

    #[tokio::test]
    async fn test_build_context() {
        let service = KnowledgeUnderstandingService::new(None);

        // Add some knowledge
        service.add_knowledge(KnowledgeItem {
            id: "k1".to_string(),
            title: "User Authentication API".to_string(),
            summary: "Authentication endpoints for users".to_string(),
            knowledge_type: KnowledgeType::ApiDocumentation,
            related_files: vec![PathBuf::from("users.rs")],
            relevance_score: 0.9,
        });

        let requirement = service.understand_requirement("Update the user authentication").await;
        let context = service.build_context(&requirement).await;

        assert!(context.confidence > 0.0);
        assert!(!context.suggested_actions.is_empty());
    }

    #[tokio::test]
    async fn test_extract_entities() {
        let service = KnowledgeUnderstandingService::new(None);

        let entities = service.extract_entities("Modify the function calculateTotal in finance.py");

        assert!(entities.iter().any(|e| e.name.contains("finance.py")));
        assert!(entities.iter().any(|e| e.name.contains("calculateTotal")));
    }

    #[tokio::test]
    async fn test_extract_constraints() {
        let service = KnowledgeUnderstandingService::new(None);

        let constraints = service.extract_constraints("Must be secure and fast");

        assert!(constraints.iter().any(|c| c.constraint_type == ConstraintType::Security));
        assert!(constraints.iter().any(|c| c.constraint_type == ConstraintType::Performance));
    }

    #[tokio::test]
    async fn test_quality_assessment() {
        let service = KnowledgeUnderstandingService::new(None);

        let requirement = service
            .understand_requirement("Create API endpoint in main.rs - must be secure")
            .await;

        assert!(requirement.quality.overall > 0.0);
    }
}
