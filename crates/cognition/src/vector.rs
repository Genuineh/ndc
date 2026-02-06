//! Vector Search - 语义搜索模块
//!
//! 职责：
//! - SimHash 轻量级向量计算
//! - 语义相似度搜索
//! - L2 索引管理
//!
//! 设计原则：
//! - 使用 SimHash 而非重型 embedding 模型
//! - 支持分页查询
//! - 与持久化层集成

use crate::memory::MemoryStore;
use ndc_core::{Memory, MemoryId, ScoredMemory, MemoryQuery};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, warn};

/// 向量搜索错误
#[derive(Debug, Error)]
pub enum VectorSearchError {
    #[error("索引不存在")]
    IndexNotFound,

    #[error("搜索失败: {0}")]
    SearchFailed(String),

    #[error("索引构建失败: {0}")]
    IndexBuildFailed(String),
}

/// SimHash 配置
#[derive(Debug, Clone)]
pub struct SimHashConfig {
    /// 生成的特征位数
    pub features: usize,

    /// Top-K 返回数量
    pub top_k: usize,

    /// 相似度阈值 (0.0 - 1.0)
    pub similarity_threshold: f64,
}

impl Default for SimHashConfig {
    fn default() -> Self {
        Self {
            features: 64,
            top_k: 10,
            similarity_threshold: 0.5,
        }
    }
}

/// SimHash 索引项
#[derive(Debug, Clone)]
struct SimHashEntry {
    /// 记忆 ID
    memory_id: MemoryId,

    /// SimHash 值
    hash: u64,

    /// 内容（用于显示）
    preview: String,
}

/// SimHash 索引
#[derive(Debug)]
pub struct SimHashIndex {
    /// 记忆存储引用
    memory_store: Arc<MemoryStore>,

    /// 索引项
    entries: Vec<SimHashEntry>,

    /// 配置
    config: SimHashConfig,
}

impl SimHashIndex {
    /// 创建新的 SimHash 索引
    pub fn new(memory_store: Arc<MemoryStore>, config: SimHashConfig) -> Self {
        Self {
            memory_store,
            entries: Vec::new(),
            config,
        }
    }

    /// 计算文本的 SimHash
    pub fn compute(text: &str) -> u64 {
        // 分词
        let tokens = Self::tokenize(text);

        // 计算每个词的哈希
        let mut hashes: Vec<u64> = tokens
            .iter()
            .map(|t| Self::hash_word(t))
            .collect();

        // 合并哈希（按位相加）
        let mut combined = vec![0i32; 64];
        for h in &hashes {
            for i in 0..64 {
                if (h >> i) & 1 == 1 {
                    combined[i] += 1;
                } else {
                    combined[i] -= 1;
                }
            }
        }

        // 转换为哈希值
        let mut result = 0u64;
        for i in 0..64 {
            if combined[i] > 0 {
                result |= 1 << i;
            }
        }

        result
    }

    /// 分词
    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty() && s.len() > 2)
            .map(|s| s.to_string())
            .collect()
    }

    /// 计算词的哈希
    fn hash_word(word: &str) -> u64 {
        // 简单哈希函数
        let mut hash = 0u64;
        for (i, c) in word.bytes().enumerate() {
            hash ^= (c as u64) << (i % 56);
        }
        hash
    }

    /// 计算两个 SimHash 的海明距离
    pub fn hamming_distance(hash1: u64, hash2: u64) -> u32 {
        (hash1 ^ hash2).count_ones()
    }

    /// 计算相似度 (1 - 海明距离/最大距离)
    pub fn similarity(hash1: u64, hash2: u64) -> f64 {
        let distance = Self::hamming_distance(hash1, hash2);
        1.0 - (distance as f64 / 64.0)
    }

    /// 向索引添加记忆
    pub async fn add_memory(&mut self, memory: &Memory) -> Result<(), VectorSearchError> {
        // 生成预览
        let preview = if memory.content.body.len() > 100 {
            memory.content.body[..100].to_string()
        } else {
            memory.content.body.clone()
        };

        // 计算 SimHash
        let hash = Self::compute(&memory.content.body);

        self.entries.push(SimHashEntry {
            memory_id: memory.id,
            hash,
            preview,
        });

        debug!("Added memory {} to index (hash: {:016x})", memory.id, hash);

        Ok(())
    }

    /// 从索引移除记忆
    pub async fn remove_memory(&mut self, memory_id: &MemoryId) {
        self.entries.retain(|e| &e.memory_id != memory_id);
    }

    /// 重建索引
    pub async fn rebuild(&mut self) -> Result<(), VectorSearchError> {
        warn!("Rebuilding SimHash index...");

        // 清空索引
        self.entries.clear();

        // 重新添加所有记忆
        let memories = self.memory_store.get_by_stability(
            ndc_core::MemoryStability::Ephemeral,
        ).await
            .map_err(|e| VectorSearchError::IndexBuildFailed(e.to_string()))?;

        for memory in memories {
            self.add_memory(&memory).await?;
        }

        info!("Rebuilt index with {} entries", self.entries.len());

        Ok(())
    }

    /// 搜索相似记忆
    pub async fn search(
        &self,
        query: &str,
        _limit: u64,
    ) -> Result<Vec<ScoredMemory>, VectorSearchError> {
        // 计算查询的 SimHash
        let query_hash = Self::compute(query);

        // 计算相似度
        let mut scores: Vec<(MemoryId, f64, String)> = self.entries
            .iter()
            .map(|e| {
                let similarity = Self::similarity(query_hash, e.hash);
                (e.memory_id, similarity, e.preview.clone())
            })
            .filter(|(_, s, _)| *s >= self.config.similarity_threshold)
            .collect();

        // 排序
        scores.sort_by(|(_, s1, _), (_, s2, _)| s2.partial_cmp(s1).unwrap());

        // 限制数量
        scores.truncate(self.config.top_k);

        // 转换为 ScoredMemory
        let mut results = Vec::new();
        for (id, score, preview) in scores {
            results.push(ScoredMemory {
                memory_id: id,
                score,
                preview,
            });
        }

        debug!("Found {} similar memories", results.len());

        Ok(results)
    }

    /// 批量获取记忆
    pub async fn get_memories(
        &self,
        memory_ids: &[MemoryId],
    ) -> Result<Vec<Memory>, VectorSearchError> {
        let mut results = Vec::new();
        for id in memory_ids {
            if let Some(memory) = self.memory_store.get(id).await
                .map_err(|e| VectorSearchError::SearchFailed(e.to_string()))? {
                results.push(memory);
            }
        }
        Ok(results)
    }
}

/// 语义搜索服务
#[derive(Debug)]
pub struct VectorSearch {
    /// 记忆存储
    memory_store: Arc<MemoryStore>,

    /// SimHash 索引
    index: Arc<std::sync::Mutex<SimHashIndex>>,

    /// 配置
    config: SimHashConfig,
}

impl VectorSearch {
    /// 创建新的语义搜索服务
    pub fn new(memory_store: Arc<MemoryStore>, config: SimHashConfig) -> Self {
        let index = SimHashIndex::new(memory_store.clone(), config.clone());
        Self {
            memory_store,
            index: Arc::new(std::sync::Mutex::new(index)),
            config,
        }
    }

    /// 创建默认配置的服务
    pub fn default(memory_store: Arc<MemoryStore>) -> Self {
        Self::new(memory_store, SimHashConfig::default())
    }

    /// 索引新记忆
    pub async fn index(&self, memory: &Memory) {
        let mut index = self.index.lock().unwrap();
        index.add_memory(memory).await
            .unwrap_or_else(|e| warn!("Failed to index memory: {}", e));
    }

    /// 移除索引
    pub async fn unindex(&self, memory_id: &MemoryId) {
        let mut index = self.index.lock().unwrap();
        index.remove_memory(memory_id).await;
    }

    /// 重建索引
    pub async fn rebuild(&self) {
        let mut index = self.index.lock().unwrap();
        index.rebuild().await
            .unwrap_or_else(|e| warn!("Failed to rebuild index: {}", e));
    }

    /// 语义搜索
    pub async fn search(&self, query: &str, limit: u64) -> Result<Vec<Memory>, VectorSearchError> {
        let index = self.index.lock().unwrap();
        let scored = index.search(query, limit).await?;

        // 获取完整记忆
        let memory_ids: Vec<MemoryId> = scored.iter().map(|s| s.memory_id).collect();
        let memories = index.get_memories(&memory_ids).await?;

        Ok(memories)
    }

    /// 搜索并返回带分数的结果
    pub async fn search_with_scores(
        &self,
        query: &str,
        limit: u64,
    ) -> Result<Vec<ScoredMemory>, VectorSearchError> {
        let index = self.index.lock().unwrap();
        let scored = index.search(query, limit).await?;

        // 获取完整记忆
        let memory_ids: Vec<MemoryId> = scored.iter().map(|s| s.memory_id).collect();
        let memories = index.get_memories(&memory_ids).await?;

        // 合并结果
        let mut results = Vec::new();
        for (scored, memory) in scored.into_iter().zip(memories) {
            results.push(ScoredMemory {
                memory_id: scored.memory_id,
                score: scored.score,
                preview: memory.content.body[..100].to_string(),
            });
        }

        Ok(results)
    }
}
