//! Storage - Simple in-memory storage
//!
//! Provides basic task and memory persistence during execution

use ndc_core::{Task, TaskId, MemoryEntry, MemoryId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use async_trait::async_trait;

/// Storage trait for task and memory persistence
#[async_trait]
pub trait Storage: Send + Sync {
    async fn save_task(&self, task: &Task) -> Result<(), String>;
    async fn get_task(&self, task_id: &TaskId) -> Result<Option<Task>, String>;
    async fn list_tasks(&self) -> Result<Vec<Task>, String>;
    async fn save_memory(&self, memory: &MemoryEntry) -> Result<(), String>;
    async fn get_memory(&self, memory_id: &MemoryId) -> Result<Option<MemoryEntry>, String>;
}

/// In-memory storage implementation
#[derive(Debug, Default)]
pub struct MemoryStorage {
    tasks: Mutex<HashMap<TaskId, Task>>,
    memories: Mutex<HashMap<MemoryId, MemoryEntry>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn save_task(&self, task: &Task) -> Result<(), String> {
        let mut tasks = self.tasks.lock().map_err(|e| e.to_string())?;
        tasks.insert(task.id, task.clone());
        Ok(())
    }

    async fn get_task(&self, task_id: &TaskId) -> Result<Option<Task>, String> {
        let tasks = self.tasks.lock().map_err(|e| e.to_string())?;
        Ok(tasks.get(task_id).cloned())
    }

    async fn list_tasks(&self) -> Result<Vec<Task>, String> {
        let tasks = self.tasks.lock().map_err(|e| e.to_string())?;
        Ok(tasks.values().cloned().collect())
    }

    async fn save_memory(&self, memory: &MemoryEntry) -> Result<(), String> {
        let mut memories = self.memories.lock().map_err(|e| e.to_string())?;
        memories.insert(memory.id, memory.clone());
        Ok(())
    }

    async fn get_memory(&self, memory_id: &MemoryId) -> Result<Option<MemoryEntry>, String> {
        let memories = self.memories.lock().map_err(|e| e.to_string())?;
        Ok(memories.get(memory_id).cloned())
    }
}

/// Shared storage reference
pub type SharedStorage = Arc<dyn Storage>;

/// Create a new shared in-memory storage
pub fn create_memory_storage() -> SharedStorage {
    Arc::new(MemoryStorage::new())
}
