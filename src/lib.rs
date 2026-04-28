//! Graph storage trait and `Neo4j` implementation.
//!
//! This crate provides the [`GraphStore`] trait for abstract graph operations
//! and a [`Neo4jStore`] implementation backed by `Neo4j` via Bolt protocol.
//!
//! All operations are append-only: entities are immutable once created.

pub mod error;
pub mod neo4j;
pub mod store;

pub use error::GraphError;
pub use neo4j::Neo4jStore;
pub use store::GraphStore;
