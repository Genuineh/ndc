// NDC Storage Layer
//
// Abstract storage interface with pluggable backends

pub mod memory;
pub mod trait_;

#[cfg(feature = "sqlite")]
pub mod sqlite;

pub use memory::{MemoryStorage, create_memory_storage};
pub use trait_::*;

#[cfg(feature = "sqlite")]
pub use sqlite::{SqliteStorage, SqliteStorageError, create_sqlite_storage};
