//! 存储抽象层
//!
//! 设计原则：
//! - 支持事务（Transaction）
//! - 支持原子写入（Atomic Write）
//! - 懒加载：使用 Stream 或分页参数，避免万级 Memory 吃内存
//! - 零拷贝读取：返回引用

use crate::core::{Task, TaskId, Memory, MemoryId, MemoryQuery, ScoredMemory};
use std::path::PathBuf;
use futures::Stream;

/// 存储错误
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("序列化错误: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("记录不存在: {0}")]
    NotFound(String),

    #[error("事务冲突: {0}")]
    TransactionConflict(String),

    #[error("并发锁定失败: {0}")]
    LockFailed(String),

    #[error("无效参数: {0}")]
    InvalidArgument(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// 存储抽象 Trait
#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    /// 打开存储
    async fn open(path: &PathBuf) -> Result<Self>
    where
        Self: Sized;

    /// 关闭存储
    async fn close(&mut self) -> Result<()>;

    // ============ Task 操作 ============

    /// 保存任务（原子写入）
    async fn save_task(&self, task: &Task) -> Result<()>;

    /// 获取任务
    async fn get_task(&self, id: &TaskId) -> Result<Option<Task>>;

    /// 获取任务（懒加载，只返回引用）
    async fn get_task_ref(&self, id: &TaskId) -> Result<Option<std::borrow::Cow<'_, Task>>>;

    /// 删除任务
    async fn delete_task(&self, id: &TaskId) -> Result<()>;

    /// 列出所有任务（带分页）
    async fn list_tasks(&self, offset: u64, limit: u64) -> Result<Vec<TaskId>>;

    /// 统计任务数量
    async fn count_tasks(&self) -> Result<u64>;

    // ============ Memory 操作 ============

    /// 保存记忆
    async fn save_memory(&self, memory: &Memory) -> Result<()>;

    /// 获取记忆
    async fn get_memory(&self, id: &MemoryId) -> Result<Option<Memory>>;

    /// 获取记忆（懒加载）
    async fn get_memory_ref(&self, id: &MemoryId) -> Result<Option<std::borrow::Cow<'_, Memory>>>;

    /// 删除记忆
    async fn delete_memory(&self, id: &MemoryId) -> Result<()>;

    // ============ 搜索操作（支持懒加载） ============

    /// 搜索记忆（返回 Stream，避免 Vec 吃内存）
    fn search_memory_stream(
        &self,
        query: &str,
        min_stability: Option<core::MemoryStability>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Memory>> + Send>>>;

    /// 搜索记忆（分页查询）
    async fn search_memory_paged(
        &self,
        query: &MemoryQuery,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<Memory>>;

    /// 统计符合条件的记忆数量
    async fn count_memory(&self, query: &MemoryQuery) -> Result<u64>;

    /// 语义搜索（向量检索，#Issue 3）
    async fn vector_search(
        &self,
        query: &str,
        limit: u64,
    ) -> Result<Vec<ScoredMemory>>;

    // ============ 事务支持 ============

    /// 开始事务
    async fn begin_transaction(&self) -> Result<Transaction>;

    /// 提交所有挂起的写入
    async fn commit(&self) -> Result<()>;

    /// 回滚所有挂起的写入
    async fn rollback(&self) -> Result<()>;
}

/// 事务
#[async_trait::async_trait]
pub trait Transaction: Send {
    /// 保存任务
    async fn save_task(&mut self, task: &Task) -> Result<()>;

    /// 删除任务
    async fn delete_task(&mut self, id: &TaskId) -> Result<()>;

    /// 保存记忆
    async fn save_memory(&mut self, memory: &Memory) -> Result<()>;

    /// 删除记忆
    async fn delete_memory(&mut self, id: &MemoryId) -> Result<()>;

    /// 提交事务
    async fn commit(self) -> Result<()>;

    /// 回滚事务
    async fn rollback(self) -> Result<()>;
}

/// 批处理操作（性能优化）
#[async_trait::async_trait]
pub trait BatchStorage: Storage {
    /// 批量保存任务
    async fn save_tasks_batch(&self, tasks: &[Task]) -> Result<()>;

    /// 批量保存记忆
    async fn save_memories_batch(&self, memories: &[Memory]) -> Result<()>;
}

/// 存储配置
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// 存储路径
    pub path: PathBuf,

    /// 最大缓存大小
    pub max_cache_size: usize,

    /// 自动压缩间隔（秒）
    pub auto_compact_interval: u64,

    /// 是否启用 WAL 模式
    pub enable_wal: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from(".ndc/storage"),
            max_cache_size: 1000,
            auto_compact_interval: 300,
            enable_wal: true,
        }
    }
}

/// 存储后端类型
#[derive(Debug, Clone)]
pub enum StorageBackend {
    /// JSON 文件存储（轻量，默认）
    Json {
        path: PathBuf,
        config: StorageConfig,
    },

    /// SQLite 存储（高性能，推荐生产）
    Sqlite {
        path: PathBuf,
        config: StorageConfig,
    },
}

impl StorageBackend {
    /// 创建 JSON 后端
    pub fn json(path: impl Into<PathBuf>) -> Self {
        Self::Json {
            path: path.into(),
            config: StorageConfig::default(),
        }
    }

    /// 创建 SQLite 后端
    pub fn sqlite(path: impl Into<PathBuf>) -> Self {
        Self::Sqlite {
            path: path.into(),
            config: StorageConfig::default(),
        }
    }
}
