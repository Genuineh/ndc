//! NDC Cognition - 认知网络模块
//!
//! 职责：
//! - 记忆存储与检索
//! - 语义搜索（SimHash 轻量级向量检索）
//! - 记忆稳定性管理
//! - 上下文组装
//!
//! 架构：
//! - MemoryStore: 记忆存储
//! - VectorSearch: 语义搜索
//! - StabilityManager: 稳定性管理
//! - ContextBuilder: 上下文组装

pub mod memory;
pub mod vector;
pub mod stability;
pub mod context;

pub use memory::{MemoryStore, MemoryEntry};
pub use vector::{VectorSearch, SimHashIndex, ScoredMemory};
pub use stability::{StabilityManager, MemoryStability};
pub use context::{ContextBuilder, ContextConfig};
