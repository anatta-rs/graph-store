//! Graph storage trait and pluggable backend implementations.
//!
//! This crate provides the [`GraphStore`] trait for abstract graph operations
//! and pluggable implementations (Neo4j, in-memory) via feature flags.
//!
//! All operations are append-only: entities are immutable once created.

pub mod error;
pub mod store;

#[cfg(feature = "neo4j-backend")]
/// Backend implementations of [`GraphStore`].
pub mod backends {
    /// Neo4j Bolt-protocol backend.
    pub mod neo4j;
    pub use self::neo4j::Neo4jStore;
}

#[cfg(feature = "neo4j-backend")]
pub use backends::Neo4jStore;

pub use error::GraphError;
pub use store::GraphStore;
