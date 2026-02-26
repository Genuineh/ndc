//! In-memory storage implementation
//!
//! Provides basic task and memory persistence during execution

use async_trait::async_trait;
use ndc_core::{MemoryEntry, MemoryId, Task, TaskId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::trait_::{SharedStorage, Storage};

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

/// Create a new shared in-memory storage
pub fn create_memory_storage() -> SharedStorage {
    Arc::new(MemoryStorage::new())
}
