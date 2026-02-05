//! JSON 文件存储实现
//!
//! 轻量级存储方案，适合开发和小规模使用。

use crate::store::{
    Storage, StorageError, Result, Transaction, BatchStorage,
    StorageConfig,
};
use crate::core::{Task, TaskId, Memory, MemoryId, MemoryQuery, ScoredMemory};
use tokio::fs::{self, File};
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use std::path::{PathBuf, Path};
use std::collections::HashMap;
use std::sync::RwLock;
use futures::Stream;
use std::pin::Pin;
use std::collections::hash_map::Entry;

/// JSON 存储实现
#[derive(Debug)]
pub struct JsonStorage {
    /// 存储根目录
    root: PathBuf,

    /// 任务缓存
    tasks: RwLock<HashMap<TaskId, Task>>,

    /// 记忆缓存
    memories: RwLock<HashMap<MemoryId, Memory>>,

    /// 配置
    config: StorageConfig,
}

impl JsonStorage {
    /// 创建新的 JSON 存储
    pub async fn new(root: PathBuf, config: StorageConfig) -> Result<Self> {
        // 创建目录结构
        let tasks_dir = root.join("tasks");
        let memories_dir = root.join("memories");

        tokio::fs::create_dir_all(&tasks_dir).await?;
        tokio::fs::create_dir_all(&memories_dir).await?;

        // 加载缓存
        let tasks = Self::load_tasks(&tasks_dir).await?;
        let memories = Self::load_memories(&memories_dir).await?;

        Ok(Self {
            root,
            tasks: RwLock::new(tasks),
            memories: RwLock::new(memories),
            config,
        })
    }

    /// 加载任务
    async fn load_tasks(dir: &Path) -> Result<HashMap<TaskId, Task>> {
        let mut map = HashMap::new();

        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_file() {
                let path = entry.path();
                if let Some(id) = Self::parse_task_id(&path) {
                    match Self::load_task(&path).await {
                        Ok(task) => { map.insert(id, task); }
                        Err(e) => tracing::warn!("Failed to load task {}: {}", id, e),
                    }
                }
            }
        }

        Ok(map)
    }

    /// 加载单个任务
    async fn load_task(path: &Path) -> Result<Task> {
        let content = tokio::fs::read_to_string(path).await?;
        serde_json::from_str(&content).map_err(StorageError::Serialize)
    }

    /// 解析任务 ID
    fn parse_task_id(path: &Path) -> Option<TaskId> {
        path.file_stem()?
            .to_str()?
            .parse::<u64>()
            .ok()
            .map(ulid::Ulid)
    }

    /// 加载记忆
    async fn load_memories(dir: &Path) -> Result<HashMap<MemoryId, Memory>> {
        let mut map = HashMap::new();

        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_file() {
                let path = entry.path();
                if let Some(id) = Self::parse_memory_id(&path) {
                    match Self::load_memory(&path).await {
                        Ok(memory) => { map.insert(id, memory); }
                        Err(e) => tracing::warn!("Failed to load memory {}: {}", id, e),
                    }
                }
            }
        }

        Ok(map)
    }

    /// 加载单个记忆
    async fn load_memory(path: &Path) -> Result<Memory> {
        let content = tokio::fs::read_to_string(path).await?;
        serde_json::from_str(&content).map_err(StorageError::Serialize)
    }

    /// 解析记忆 ID
    fn parse_memory_id(path: &Path) -> Option<MemoryId> {
        path.file_stem()?
            .to_str()?
            .parse::<u64>()
            .ok()
            .map(ulid::Ulid)
    }

    /// 获取任务文件路径
    fn task_path(&self, id: &TaskId) -> PathBuf {
        self.root.join("tasks").join(id.to_string())
    }

    /// 获取记忆文件路径
    fn memory_path(&self, id: &MemoryId) -> PathBuf {
        self.root.join("memories").join(id.to_string())
    }
}

#[async_trait::async_trait]
impl Storage for JsonStorage {
    async fn open(_path: &PathBuf) -> Result<Self>
    where
        Self: Sized,
    {
        unimplemented!("Use JsonStorage::new() instead")
    }

    async fn close(&mut self) -> Result<()> {
        // 刷新所有挂起的更改
        self.commit().await?;
        Ok(())
    }

    // ============ Task 操作 ============

    async fn save_task(&self, task: &Task) -> Result<()> {
        let path = self.task_path(&task.id);
        let content = serde_json::to_string_pretty(task)
            .map_err(StorageError::Serialize)?;

        // 先写入临时文件
        let temp_path = path.with_extension("tmp");
        tokio::fs::write(&temp_path, &content).await?;

        // 重命名为正式文件（原子操作）
        tokio::fs::rename(&temp_path, &path).await?;

        // 更新缓存
        {
            let mut tasks = self.tasks.write().unwrap();
            tasks.insert(task.id.clone(), task.clone());
        }

        Ok(())
    }

    async fn get_task(&self, id: &TaskId) -> Result<Option<Task>> {
        // 先查缓存
        {
            let tasks = self.tasks.read().unwrap();
            if let Some(task) = tasks.get(id) {
                return Ok(Some(task.clone()));
            }
        }

        // 缓存未命中，从文件加载
        let path = self.task_path(id);
        if path.exists() {
            let task = Self::load_task(&path).await?;
            let mut tasks = self.tasks.write().unwrap();
            tasks.insert(id.clone(), task.clone());
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    async fn get_task_ref(&self, id: &TaskId) -> Result<Option<std::borrow::Cow<'_, Task>>> {
        let tasks = self.tasks.read().unwrap();
        Ok(tasks.get(id).map(|t| std::borrow::Cow::Borrowed(t)))
    }

    async fn delete_task(&self, id: &TaskId) -> Result<()> {
        let path = self.task_path(id);

        // 删除文件
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }

        // 更新缓存
        let mut tasks = self.tasks.write().unwrap();
        tasks.remove(id);

        Ok(())
    }

    async fn list_tasks(&self, offset: u64, limit: u64) -> Result<Vec<TaskId>> {
        let tasks = self.tasks.read().unwrap();
        let ids: Vec<TaskId> = tasks.keys().cloned().collect();

        // 排序（按创建时间）
        let mut ids: Vec<_> = ids.iter().collect();
        ids.sort_by_key(|id| id);

        let end = std::cmp::min((offset + limit) as usize, ids.len());
        Ok(ids[offset as usize..end].iter().cloned().collect())
    }

    async fn count_tasks(&self) -> Result<u64> {
        let tasks = self.tasks.read().unwrap();
        Ok(tasks.len() as u64)
    }

    // ============ Memory 操作 ============

    async fn save_memory(&self, memory: &Memory) -> Result<()> {
        let path = self.memory_path(&memory.id);
        let content = serde_json::to_string_pretty(memory)
            .map_err(StorageError::Serialize)?;

        // 先写入临时文件
        let temp_path = path.with_extension("tmp");
        tokio::fs::write(&temp_path, &content).await?;

        // 重命名为正式文件（原子操作）
        tokio::fs::rename(&temp_path, &path).await?;

        // 更新缓存
        {
            let mut memories = self.memories.write().unwrap();
            memories.insert(memory.id.clone(), memory.clone());
        }

        Ok(())
    }

    async fn get_memory(&self, id: &MemoryId) -> Result<Option<Memory>> {
        // 先查缓存
        {
            let memories = self.memories.read().unwrap();
            if let Some(memory) = memories.get(id) {
                return Ok(Some(memory.clone()));
            }
        }

        // 缓存未命中，从文件加载
        let path = self.memory_path(id);
        if path.exists() {
            let memory = Self::load_memory(&path).await?;
            let mut memories = self.memories.write().unwrap();
            memories.insert(id.clone(), memory.clone());
            Ok(Some(memory))
        } else {
            Ok(None)
        }
    }

    async fn get_memory_ref(&self, id: &MemoryId) -> Result<Option<std::borrow::Cow<'_, Memory>>> {
        let memories = self.memories.read().unwrap();
        Ok(memories.get(id).map(|m| std::borrow::Cow::Borrowed(m)))
    }

    async fn delete_memory(&self, id: &MemoryId) -> Result<()> {
        let path = self.memory_path(id);

        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }

        let mut memories = self.memories.write().unwrap();
        memories.remove(id);

        Ok(())
    }

    // ============ 搜索操作 ============

    fn search_memory_stream(
        &self,
        _query: &str,
        _min_stability: Option<core::MemoryStability>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Memory>> + Send>>> {
        // 简化实现：返回所有记忆的 Stream
        let memories = {
            let memories = self.memories.read().unwrap();
            memories.values().cloned().collect::<Vec<_>>()
        };

        let stream = futures::stream::iter(memories.into_iter().map(Ok));
        Ok(Box::pin(stream))
    }

    async fn search_memory_paged(
        &self,
        query: &MemoryQuery,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<Memory>> {
        let memories = self.memories.read().unwrap();
        let mut results: Vec<&Memory> = memories.values().collect();

        // 过滤
        if let Some(min_stability) = query.min_stability {
            results.retain(|m| m.stability >= min_stability);
        }

        if let Some(memory_type) = &query.memory_type {
            results.retain(|m| m.memory_type == *memory_type);
        }

        if !query.tags.is_empty() {
            results.retain(|m| {
                query.tags.iter().all(|tag| m.content.tags.contains(tag))
            });
        }

        // 排序
        results.sort_by_key(|m| &m.id);

        // 分页
        let end = std::cmp::min((offset + limit) as usize, results.len());
        Ok(results[offset as usize..end].iter().map(|m| m.clone()).collect())
    }

    async fn count_memory(&self, query: &MemoryQuery) -> Result<u64> {
        let memories = self.memories.read().unwrap();
        let mut count = 0;

        for memory in memories.values() {
            if let Some(min_stability) = query.min_stability {
                if memory.stability < min_stability {
                    continue;
                }
            }

            if let Some(memory_type) = &query.memory_type {
                if memory.memory_type != *memory_type {
                    continue;
                }
            }

            if !query.tags.is_empty() {
                if !query.tags.iter().all(|tag| memory.content.tags.contains(tag)) {
                    continue;
                }
            }

            count += 1;
        }

        Ok(count)
    }

    async fn vector_search(
        &self,
        _query: &str,
        _limit: u64,
    ) -> Result<Vec<ScoredMemory>> {
        // TODO: 实现向量检索（#Issue 3）
        // 目前返回空结果
        Ok(vec![])
    }

    // ============ 事务支持 ============

    async fn begin_transaction(&self) -> Result<Transaction> {
        Ok(JsonTransaction::new(self))
    }

    async fn commit(&self) -> Result<()> {
        // JSON 存储不支持自动提交
        // 所有写入已通过 save_task/save_memory 直接持久化
        Ok(())
    }

    async fn rollback(&self) -> Result<()> {
        // JSON 存储不支持自动回滚
        // 需要实现完整的事务语义
        Ok(())
    }
}

/// JSON 事务
#[derive(Debug)]
pub struct JsonTransaction<'a> {
    storage: &'a JsonStorage,
    tasks: HashMap<TaskId, Option<Task>>,
    memories: HashMap<MemoryId, Option<Memory>>,
}

impl<'a> JsonTransaction<'a> {
    fn new(storage: &'a JsonStorage) -> Self {
        Self {
            storage,
            tasks: HashMap::new(),
            memories: HashMap::new(),
        }
    }
}

#[async_trait::async_trait]
impl<'a> Transaction for JsonTransaction<'a> {
    async fn save_task(&mut self, task: &Task) -> Result<()> {
        self.tasks.insert(task.id.clone(), Some(task.clone()));
        Ok(())
    }

    async fn delete_task(&mut self, id: &TaskId) -> Result<()> {
        self.tasks.insert(id.clone(), None);
        Ok(())
    }

    async fn save_memory(&mut self, memory: &Memory) -> Result<()> {
        self.memories.insert(memory.id.clone(), Some(memory.clone()));
        Ok(())
    }

    async fn delete_memory(&mut self, id: &MemoryId) -> Result<()> {
        self.memories.insert(id.clone(), None);
        Ok(())
    }

    async fn commit(mut self) -> Result<()> {
        // 提交任务
        for (id, task) in self.tasks.into_iter() {
            match task {
                Some(t) => self.storage.save_task(&t).await?,
                None => self.storage.delete_task(&id).await?,
            }
        }

        // 提交记忆
        for (id, memory) in self.memories.into_iter() {
            match memory {
                Some(m) => self.storage.save_memory(&m).await?,
                None => self.storage.delete_memory(&id).await?,
            }
        }

        Ok(())
    }

    async fn rollback(self) -> Result<()> {
        // 事务回滚 - 丢弃所有更改
        Ok(())
    }
}

/// 创建 JSON 存储的便捷函数
pub async fn create_json_storage(
    path: impl Into<PathBuf>,
) -> Result<JsonStorage> {
    JsonStorage::new(path.into(), StorageConfig::default()).await
}
