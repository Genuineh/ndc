//! LLM integration module

pub mod decomposition;
pub mod selector;
pub mod provider;
pub mod understanding;

pub use decomposition::*;
pub use selector::ModelSelector;
pub use provider::*;
// Exclude RelationType to avoid conflict with memory::RelationType
pub use understanding::{
    KnowledgeUnderstandingService,
    UnderstandingConfig,
    UnderstandingContext,
    Requirement,
    RequirementIntent,
    Entity,
    EntityType,
    Relationship,
    Constraint,
    ConstraintType,
    RequirementQuality,
    KnowledgeItem,
};
