//! Graph storage trait and pluggable backend implementations.
//!
//! This crate provides the [`GraphStore`] trait for abstract graph operations
//! and pluggable implementations (Neo4j, in-memory) via feature flags.
//!
//! All operations are append-only: entities are immutable once created.

pub mod error;
pub mod store;

/// Backend implementations of [`GraphStore`].
pub mod backends {
    /// Neo4j Bolt-protocol backend.
    #[cfg(feature = "neo4j-backend")]
    pub mod neo4j;
    #[cfg(feature = "neo4j-backend")]
    pub use self::neo4j::Neo4jStore;

    /// In-memory backend for tests and dev tooling.
    #[cfg(feature = "memory-backend")]
    pub mod memory;
    #[cfg(feature = "memory-backend")]
    pub use self::memory::MemoryStore;
}

#[cfg(feature = "neo4j-backend")]
pub use backends::Neo4jStore;

#[cfg(feature = "memory-backend")]
pub use backends::MemoryStore;

pub use error::GraphError;
pub use store::GraphStore;
