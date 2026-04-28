//! [`GraphStore`] trait -- the abstract graph interface.
//!
//! This is a simple CRUD + query interface. No business logic.
//! All entities are immutable: create only, never update.

use std::future::Future;

use crate::GraphError;

/// Abstract async graph store.
///
/// Implementations must be `Send + Sync` for use across async tasks.
/// All operations are append-only: entities and relationships are
/// immutable once created.
pub trait GraphStore: Send + Sync {
    /// Execute a raw Cypher query and return results as JSON.
    fn execute_cypher(
        &self,
        query: &str,
        params: serde_json::Value,
    ) -> impl Future<Output = Result<Vec<serde_json::Value>, GraphError>> + Send;

    /// Create a node with the given properties.
    fn create_node(
        &self,
        id: &str,
        labels: &[&str],
        properties: serde_json::Value,
    ) -> impl Future<Output = Result<(), GraphError>> + Send;

    /// Create a relationship between two nodes.
    ///
    /// Always appends a new edge. Use for **tick-edges** whose multiplicity
    /// carries meaning: `ACTIVATES`, `INHIBITS`, `CAUSES` — each occurrence
    /// is an event, conviction counts edges.
    fn create_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
        properties: serde_json::Value,
    ) -> impl Future<Output = Result<(), GraphError>> + Send;

    /// Create or update an idempotent relationship between two nodes.
    ///
    /// If an edge of the given `rel_type` already exists from `source_id`
    /// to `target_id`, updates its properties. Otherwise creates it.
    ///
    /// Use for **unique-per-pair edges** whose multiplicity is a bug:
    /// `CONTAINS`, `NEXT`, `PRODUCED`, `IN_TOPOLOGY`. Calling this repeatedly
    /// with the same endpoints must leave exactly one edge in the graph.
    fn merge_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
        properties: serde_json::Value,
    ) -> impl Future<Output = Result<(), GraphError>> + Send;

    /// Get a node by ID, returning its properties as JSON.
    fn get_node(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<serde_json::Value>, GraphError>> + Send;

    /// Find nodes matching a set of property filters.
    fn find_nodes(
        &self,
        label: &str,
        filters: serde_json::Value,
    ) -> impl Future<Output = Result<Vec<serde_json::Value>, GraphError>> + Send;

    /// Get all relationships from or to a node.
    fn get_relationships(
        &self,
        node_id: &str,
        rel_type: Option<&str>,
    ) -> impl Future<Output = Result<Vec<serde_json::Value>, GraphError>> + Send;
}
