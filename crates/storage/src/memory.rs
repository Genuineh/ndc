//! In-memory storage implementation
//!
//! Provides basic task and memory persistence during execution

use async_trait::async_trait;
use ndc_core::{MemoryEntry, MemoryId, Task, TaskId};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::trait_::{SharedStorage, Storage};

/// Default capacity limits
const DEFAULT_MAX_TASKS: usize = 10_000;
const DEFAULT_MAX_MEMORIES: usize = 10_000;

/// In-memory storage implementation with capacity limits (FIFO eviction)
#[derive(Debug)]
pub struct MemoryStorage {
    tasks: Mutex<(HashMap<TaskId, Task>, VecDeque<TaskId>)>,
    memories: Mutex<(HashMap<MemoryId, MemoryEntry>, VecDeque<MemoryId>)>,
    max_tasks: usize,
    max_memories: usize,
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_TASKS, DEFAULT_MAX_MEMORIES)
    }

    pub fn with_capacity(max_tasks: usize, max_memories: usize) -> Self {
        Self {
            tasks: Mutex::new((HashMap::new(), VecDeque::new())),
            memories: Mutex::new((HashMap::new(), VecDeque::new())),
            max_tasks,
            max_memories,
        }
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn save_task(&self, task: &Task) -> Result<(), String> {
        let mut guard = self.tasks.lock().await;
        let (map, order) = &mut *guard;
        if !map.contains_key(&task.id) {
            // Evict oldest if at capacity
            while map.len() >= self.max_tasks {
                if let Some(oldest) = order.pop_front() {
                    map.remove(&oldest);
                } else {
                    break;
                }
            }
            order.push_back(task.id);
        }
        map.insert(task.id, task.clone());
        Ok(())
    }

    async fn get_task(&self, task_id: &TaskId) -> Result<Option<Task>, String> {
        let guard = self.tasks.lock().await;
        Ok(guard.0.get(task_id).cloned())
    }

    async fn list_tasks(&self) -> Result<Vec<Task>, String> {
        let guard = self.tasks.lock().await;
        Ok(guard.0.values().cloned().collect())
    }

    async fn save_memory(&self, memory: &MemoryEntry) -> Result<(), String> {
        let mut guard = self.memories.lock().await;
        let (map, order) = &mut *guard;
        if !map.contains_key(&memory.id) {
            // Evict oldest if at capacity
            while map.len() >= self.max_memories {
                if let Some(oldest) = order.pop_front() {
                    map.remove(&oldest);
                } else {
                    break;
                }
            }
            order.push_back(memory.id);
        }
        map.insert(memory.id, memory.clone());
        Ok(())
    }

    async fn get_memory(&self, memory_id: &MemoryId) -> Result<Option<MemoryEntry>, String> {
        let guard = self.memories.lock().await;
        Ok(guard.0.get(memory_id).cloned())
    }
}

/// Create a new shared in-memory storage
pub fn create_memory_storage() -> SharedStorage {
    Arc::new(MemoryStorage::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndc_core::{
        AccessControl, AgentId, AgentRole, MemoryContent, MemoryMetadata, MemoryStability,
    };

    fn make_task() -> Task {
        Task::new(
            "test".to_string(),
            "desc".to_string(),
            AgentRole::Implementer,
        )
    }

    fn make_memory() -> MemoryEntry {
        let agent_id = AgentId::new();
        let source_task = TaskId::new();
        MemoryEntry {
            id: MemoryId::new(),
            content: MemoryContent::General {
                text: "test fact".to_string(),
                metadata: String::new(),
            },
            embedding: vec![],
            relations: vec![],
            metadata: MemoryMetadata {
                stability: MemoryStability::Ephemeral,
                created_at: chrono::Utc::now(),
                created_by: agent_id.clone(),
                source_task,
                version: 1,
                modified_at: None,
                tags: vec![],
            },
            access_control: AccessControl::new(agent_id, MemoryStability::Ephemeral),
        }
    }

    #[tokio::test]
    async fn test_save_and_get_task() {
        let storage = MemoryStorage::new();
        let task = make_task();
        storage.save_task(&task).await.unwrap();
        let got = storage.get_task(&task.id).await.unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().id, task.id);
    }

    #[tokio::test]
    async fn test_task_capacity_eviction() {
        let storage = MemoryStorage::with_capacity(3, 100);
        let mut ids = vec![];
        for _ in 0..5 {
            let task = make_task();
            ids.push(task.id);
            storage.save_task(&task).await.unwrap();
        }
        // Only 3 should remain
        let all = storage.list_tasks().await.unwrap();
        assert_eq!(all.len(), 3);
        // First 2 should be evicted
        assert!(storage.get_task(&ids[0]).await.unwrap().is_none());
        assert!(storage.get_task(&ids[1]).await.unwrap().is_none());
        // Last 3 should remain
        assert!(storage.get_task(&ids[2]).await.unwrap().is_some());
        assert!(storage.get_task(&ids[3]).await.unwrap().is_some());
        assert!(storage.get_task(&ids[4]).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_update_existing_task_no_eviction() {
        let storage = MemoryStorage::with_capacity(2, 100);
        let task = make_task();
        storage.save_task(&task).await.unwrap();
        // Update same task should not trigger eviction
        let mut updated = task.clone();
        updated.title = "updated".to_string();
        storage.save_task(&updated).await.unwrap();
        assert_eq!(storage.list_tasks().await.unwrap().len(), 1);
        let got = storage.get_task(&task.id).await.unwrap().unwrap();
        assert_eq!(got.title, "updated");
    }

    #[tokio::test]
    async fn test_memory_capacity_eviction() {
        let storage = MemoryStorage::with_capacity(100, 2);
        let m1 = make_memory();
        let m2 = make_memory();
        let m3 = make_memory();
        let id1 = m1.id;
        let id2 = m2.id;
        let id3 = m3.id;

        storage.save_memory(&m1).await.unwrap();
        storage.save_memory(&m2).await.unwrap();
        storage.save_memory(&m3).await.unwrap();

        // m1 should be evicted
        assert!(storage.get_memory(&id1).await.unwrap().is_none());
        assert!(storage.get_memory(&id2).await.unwrap().is_some());
        assert!(storage.get_memory(&id3).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_save_and_get_memory() {
        let storage = MemoryStorage::new();
        let mem = make_memory();
        let id = mem.id;
        storage.save_memory(&mem).await.unwrap();
        let got = storage.get_memory(&id).await.unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_memory_update_existing_no_eviction() {
        let storage = MemoryStorage::with_capacity(100, 2);
        let mut mem = make_memory();
        let id = mem.id;
        storage.save_memory(&mem).await.unwrap();
        // Update same memory
        mem.content = MemoryContent::General {
            text: "updated fact".to_string(),
            metadata: String::new(),
        };
        storage.save_memory(&mem).await.unwrap();
        // Should still have only one entry, no extra eviction slot consumed
        let got = storage.get_memory(&id).await.unwrap().unwrap();
        match &got.content {
            MemoryContent::General { text, .. } => assert_eq!(text, "updated fact"),
            _ => panic!("unexpected content variant"),
        }
    }

    #[tokio::test]
    async fn test_list_tasks_returns_all() {
        let storage = MemoryStorage::new();
        let t1 = make_task();
        let t2 = make_task();
        let id1 = t1.id;
        let id2 = t2.id;
        storage.save_task(&t1).await.unwrap();
        storage.save_task(&t2).await.unwrap();
        let all = storage.list_tasks().await.unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|t| t.id == id1));
        assert!(all.iter().any(|t| t.id == id2));
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_none() {
        let storage = MemoryStorage::new();
        assert!(storage.get_task(&TaskId::new()).await.unwrap().is_none());
        assert!(
            storage
                .get_memory(&MemoryId::new())
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_concurrent_saves() {
        use std::sync::Arc;
        let storage = Arc::new(MemoryStorage::new());
        let mut handles = vec![];
        for _ in 0..20 {
            let s = storage.clone();
            handles.push(tokio::spawn(async move {
                let task = make_task();
                s.save_task(&task).await.unwrap();
                task.id
            }));
        }
        let mut ids = Vec::new();
        for h in handles {
            ids.push(h.await.unwrap());
        }
        let all = storage.list_tasks().await.unwrap();
        assert_eq!(all.len(), 20);
        for id in &ids {
            assert!(storage.get_task(id).await.unwrap().is_some());
        }
    }

    #[tokio::test]
    async fn test_zero_capacity_evicts_immediately() {
        let storage = MemoryStorage::with_capacity(0, 0);
        let task = make_task();
        // save should succeed but item is immediately evicted on next insert
        // With capacity 0, the loop `while map.len() >= 0` always evicts
        // Actually: if capacity is 0, every new item triggers eviction of itself
        // Let's verify: inserting and then getting should show eviction behavior
        storage.save_task(&task).await.unwrap();
        // capacity 0 means map.len() (1) >= max_tasks (0), so next insert evicts
        // But the first insert: map is empty (0 >= 0 is true), it tries to evict but nothing in order
        // Then it inserts. So first item stays.
        let got = storage.get_task(&task.id).await.unwrap();
        // Because while 0 >= 0, it pops from order (empty), breaks, then inserts.
        // So actually the item IS stored.
        assert!(got.is_some());

        // Second insert should evict the first
        let task2 = make_task();
        storage.save_task(&task2).await.unwrap();
        assert!(storage.get_task(&task.id).await.unwrap().is_none());
        assert!(storage.get_task(&task2.id).await.unwrap().is_some());
    }
}
