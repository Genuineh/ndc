//! Storage trait definition
//!
//! Abstract interface for task and memory persistence

use async_trait::async_trait;
use ndc_core::{MemoryEntry, MemoryId, Task, TaskId};
use std::sync::Arc;

/// Storage trait for task and memory persistence
#[async_trait]
pub trait Storage: Send + Sync {
    async fn save_task(&self, task: &Task) -> Result<(), String>;
    async fn get_task(&self, task_id: &TaskId) -> Result<Option<Task>, String>;
    async fn list_tasks(&self) -> Result<Vec<Task>, String>;
    async fn save_memory(&self, memory: &MemoryEntry) -> Result<(), String>;
    async fn get_memory(&self, memory_id: &MemoryId) -> Result<Option<MemoryEntry>, String>;
}

/// Shared storage reference
pub type SharedStorage = Arc<dyn Storage>;
