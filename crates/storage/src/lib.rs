// NDC Storage Layer
//
// Abstract storage interface with pluggable backends

pub mod trait_;
pub mod memory;

#[cfg(feature = "sqlite")]
pub mod sqlite;

pub use trait_::*;
pub use memory::{create_memory_storage, MemoryStorage};

#[cfg(feature = "sqlite")]
pub use sqlite::{create_sqlite_storage, SqliteStorage, SqliteStorageError};
