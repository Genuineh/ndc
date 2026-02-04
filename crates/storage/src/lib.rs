// NDC Storage Layer
//
// Abstract storage interface with pluggable backends

pub mod trait_;
pub mod memory;

#[cfg(feature = "sqlite")]
pub mod sqlite;

pub use trait_::*;
