//! Memory Store - 记忆存储模块
//!
//! 职责：
//! - 记忆的 CRUD 操作
//! - 记忆索引管理
//! - 与持久化层集成

use ndc_core::{Memory, MemoryId, MemoryContent, MemoryType, MemoryStability, MemoryMetadata};
use ndc_persistence::Storage;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, warn};

/// 记忆存储错误
#[derive(Debug, Error)]
pub enum MemoryStoreError {
    #[error("记忆不存在: {0}")]
    NotFound(MemoryId),

    #[error("记忆已存在: {0}")]
    AlreadyExists(MemoryId),

    #[error("存储错误: {0}")]
    StorageError(String),
}

/// 记忆存储
#[derive(Debug)]
pub struct MemoryStore {
    /// 存储后端
    storage: Arc<dyn Storage>,

    /// L1 缓存（工作空间级别，随 Task 销毁）
    l1_cache: Arc<lru::LruCache<MemoryId, Memory>>,

    /// 缓存配置
    cache_size: usize,
}

impl MemoryStore {
    /// 创建新的记忆存储
    pub fn new(storage: Arc<dyn Storage>, cache_size: usize) -> Self {
        Self {
            storage,
            l1_cache: Arc::new(lru::LruCache::new(cache_size)),
            cache_size,
        }
    }

    /// 创建默认配置的记忆存储
    pub fn default(storage: Arc<dyn Storage>) -> Self {
        Self::new(storage, 1000)
    }

    /// 保存记忆
    pub async fn save(&self, memory: &Memory) -> Result<(), MemoryStoreError> {
        // 检查是否已存在
        if let Some(existing) = self.storage.get_memory(&memory.id).await
            .map_err(|e| MemoryStoreError::StorageError(e.to_string()))? {
            warn!("Overwriting memory: {}", memory.id);
        }

        self.storage.save_memory(memory).await
            .map_err(|e| MemoryStoreError::StorageError(e.to_string()))?;

        // 更新 L1 缓存
        self.l1_cache.put(memory.id, memory.clone());

        debug!("Saved memory: {} ({:?})", memory.id, memory.memory_type);

        Ok(())
    }

    /// 获取记忆
    pub async fn get(&self, id: &MemoryId) -> Result<Option<Memory>, MemoryStoreError> {
        // 先查 L1 缓存
        if let Some(cached) = self.l1_cache.get(id) {
            return Ok(Some(cached.clone()));
        }

        // 查存储
        let memory = self.storage.get_memory(id).await
            .map_err(|e| MemoryStoreError::StorageError(e.to_string()))?;

        if let Some(ref mem) = memory {
            // 更新缓存
            self.l1_cache.put(*id, mem.clone());
        }

        Ok(memory)
    }

    /// 删除记忆
    pub async fn delete(&self, id: &MemoryId) -> Result<(), MemoryStoreError> {
        // 从缓存移除
        self.l1_cache.pop(id);

        self.storage.delete_memory(id).await
            .map_err(|e| MemoryStoreError::StorageError(e.to_string()))?;

        debug!("Deleted memory: {}", id);

        Ok(())
    }

    /// 按稳定性等级检索
    pub async fn get_by_stability(
        &self,
        min_stability: MemoryStability,
    ) -> Result<Vec<Memory>, MemoryStoreError> {
        let query = ndc_core::MemoryQuery {
            min_stability: Some(min_stability),
            ..Default::default()
        };

        self.storage.search_memory_paged(&query, 0, u64::MAX).await
            .map_err(|e| MemoryStoreError::StorageError(e.to_string()))
    }

    /// 按类型检索
    pub async fn get_by_type(
        &self,
        memory_type: MemoryType,
    ) -> Result<Vec<Memory>, MemoryStoreError> {
        let query = ndc_core::MemoryQuery {
            memory_type: Some(memory_type),
            ..Default::default()
        };

        self.storage.search_memory_paged(&query, 0, u64::MAX).await
            .map_err(|e| MemoryStoreError::StorageError(e.to_string()))
    }

    /// 获取所有记忆数量
    pub async fn count(&self) -> Result<u64, MemoryStoreError> {
        let query = ndc_core::MemoryQuery::default();
        self.storage.count_memory(&query)
            .await
            .map_err(|e| MemoryStoreError::StorageError(e.to_string()))
    }

    /// 清空 L1 缓存（通常在 Task 结束时调用）
    pub fn clear_l1_cache(&mut self) {
        self.l1_cache.clear();
        info!("Cleared L1 cache");
    }

    /// 提升记忆稳定性
    pub async fn promote(
        &self,
        id: &MemoryId,
        to_stability: MemoryStability,
        reason: &str,
    ) -> Result<(), MemoryStoreError> {
        let mut memory = self.get(id).await?
            .ok_or_else(|| MemoryStoreError::NotFound(*id))?;

        // 验证稳定性提升是单向的
        if memory.content.stability >= to_stability {
            warn!("Cannot promote memory to lower stability");
            return Ok(());
        }

        memory.content.stability = to_stability;
        memory.content.version += 1;

        // 记录稳定性变更历史
        memory.content.history.push(ndc_core::StabilityChange {
            from: memory.content.stability,
            to: to_stability,
            reason: reason.to_string(),
            changed_at: ndc_core::Timestamp::now(),
        });

        self.save(&memory).await?;

        info!("Promoted memory {} to {:?} ({})", id, to_stability, reason);

        Ok(())
    }
}

/// 创建新记忆的便捷函数
pub fn create_memory(
    content: String,
    memory_type: MemoryType,
    stability: MemoryStability,
    source: &str,
) -> Memory {
    let id = MemoryId::new();

    Memory {
        id,
        content: MemoryContent {
            body: content,
            memory_type,
            stability,
            tags: vec![],
            version: 1,
            last_accessed: ndc_core::Timestamp::now(),
            access_count: 0,
            source: source.to_string(),
            metadata: ndc_core::MemoryMetadata::default(),
            history: vec![],
            related_memories: vec![],
            embedding: None,
        },
        relations: vec![],
    }
}
