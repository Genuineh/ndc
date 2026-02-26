//! LLM integration module

pub mod decomposition;
pub mod provider;
pub mod selector;
pub mod understanding;

pub use decomposition::*;
pub use provider::*;
pub use selector::ModelSelector;
// Exclude RelationType to avoid conflict with memory::RelationType
pub use understanding::{
    Constraint, ConstraintType, Entity, EntityType, KnowledgeItem, KnowledgeUnderstandingService,
    Relationship, Requirement, RequirementIntent, RequirementQuality, UnderstandingConfig,
    UnderstandingContext,
};
