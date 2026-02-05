//! NDC Persistence - 持久化层
//!
//! 支持多种存储后端：
//! - JSON 文件存储（轻量，默认）
//! - SQLite 存储（高性能，推荐生产）
//!
//! 设计原则：
//! - 事务支持
//! - 原子写入
//! - 懒加载（Stream + 分页）
//! - 零拷贝读取

pub mod store;
pub mod json;

pub use store::{
    Storage, StorageError, Result, Transaction, BatchStorage,
    StorageConfig, StorageBackend,
};
