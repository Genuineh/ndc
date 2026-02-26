//! Knowledge Injectors for AI Agent
//!
//! Responsibilities:
//! - WorkingMemoryInjector: Inject current working memory context
//! - InvariantInjector: Inject Gold Memory constraints
//! - TaskLineageInjector: Inject task lineage context
//!
//! Design: Inject NDC's cognitive system into Agent prompts

pub mod invariant;
pub mod lineage;
pub mod working_memory;
