//! Error types for the graph store.

/// Errors produced by [`GraphStore`](crate::GraphStore) operations.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    /// The underlying graph backend failed.
    #[error("graph backend error: {0}")]
    Backend(String),

    /// The query was malformed or invalid.
    #[error("query error: {0}")]
    Query(String),

    /// Serialization or deserialization of graph data failed.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// The requested entity was not found.
    #[error("not found: {0}")]
    NotFound(String),
}

impl From<neo4rs::Error> for GraphError {
    fn from(err: neo4rs::Error) -> Self {
        Self::Backend(err.to_string())
    }
}
