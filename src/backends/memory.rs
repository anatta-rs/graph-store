//! In-memory [`GraphStore`](crate::GraphStore) backend.
//!
//! HashMap + Vec backed implementation. Intended for tests and dev tooling
//! that wants the trait surface without spinning up a Neo4j instance.
//!
//! `execute_cypher` returns `GraphError::Backend("memory backend does not
//! support cypher")` — the Cypher escape hatch is Neo4j-specific.

use std::collections::HashMap;
use std::sync::RwLock;

use serde_json::{Map, Value};

use crate::{GraphError, GraphStore};

/// A single edge in the in-memory graph.
#[derive(Debug, Clone)]
struct Edge {
    source_id: String,
    target_id: String,
    rel_type: String,
    properties: Value,
}

/// HashMap-backed [`GraphStore`].
///
/// All operations are append-only on nodes (no SET / no DELETE) — same
/// invariant as the Neo4j backend. `merge_relationship` is the only
/// upsert primitive.
#[derive(Default)]
pub struct MemoryStore {
    nodes: RwLock<HashMap<String, NodeRecord>>,
    edges: RwLock<Vec<Edge>>,
}

#[derive(Debug, Clone)]
struct NodeRecord {
    labels: Vec<String>,
    properties: Value,
}

impl MemoryStore {
    /// Create an empty memory store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of nodes currently stored.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.read().expect("nodes lock poisoned").len()
    }

    /// Number of edges currently stored.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.read().expect("edges lock poisoned").len()
    }
}

fn node_to_value(id: &str, labels: &[String], properties: &Value) -> Value {
    let mut obj = match properties {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    obj.insert("id".to_string(), Value::String(id.to_string()));
    obj.insert(
        "labels".to_string(),
        Value::Array(
            labels
                .iter()
                .map(|l| Value::String(l.clone()))
                .collect::<Vec<_>>(),
        ),
    );
    Value::Object(obj)
}

fn edge_to_value(edge: &Edge) -> Value {
    let mut obj = match &edge.properties {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    obj.insert(
        "source_id".to_string(),
        Value::String(edge.source_id.clone()),
    );
    obj.insert(
        "target_id".to_string(),
        Value::String(edge.target_id.clone()),
    );
    obj.insert("rel_type".to_string(), Value::String(edge.rel_type.clone()));
    Value::Object(obj)
}

fn props_match(node_props: &Value, filters: &Value) -> bool {
    let Value::Object(filters_obj) = filters else {
        return true;
    };
    let Value::Object(node_obj) = node_props else {
        return filters_obj.is_empty();
    };
    filters_obj
        .iter()
        .all(|(k, v)| node_obj.get(k).is_some_and(|nv| nv == v))
}

impl GraphStore for MemoryStore {
    async fn execute_cypher(&self, _query: &str, _params: Value) -> Result<Vec<Value>, GraphError> {
        Err(GraphError::Backend(
            "memory backend does not support cypher".to_string(),
        ))
    }

    async fn create_node(
        &self,
        id: &str,
        labels: &[&str],
        properties: Value,
    ) -> Result<(), GraphError> {
        let mut nodes = self.nodes.write().expect("nodes lock poisoned");
        nodes.insert(
            id.to_string(),
            NodeRecord {
                labels: labels.iter().map(|s| (*s).to_string()).collect(),
                properties,
            },
        );
        Ok(())
    }

    async fn create_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
        properties: Value,
    ) -> Result<(), GraphError> {
        let mut edges = self.edges.write().expect("edges lock poisoned");
        edges.push(Edge {
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            rel_type: rel_type.to_string(),
            properties,
        });
        Ok(())
    }

    async fn merge_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
        properties: Value,
    ) -> Result<(), GraphError> {
        let mut edges = self.edges.write().expect("edges lock poisoned");
        if let Some(existing) = edges.iter_mut().find(|e| {
            e.source_id == source_id && e.target_id == target_id && e.rel_type == rel_type
        }) {
            existing.properties = properties;
        } else {
            edges.push(Edge {
                source_id: source_id.to_string(),
                target_id: target_id.to_string(),
                rel_type: rel_type.to_string(),
                properties,
            });
        }
        Ok(())
    }

    async fn get_node(&self, id: &str) -> Result<Option<Value>, GraphError> {
        let nodes = self.nodes.read().expect("nodes lock poisoned");
        Ok(nodes
            .get(id)
            .map(|n| node_to_value(id, &n.labels, &n.properties)))
    }

    async fn find_nodes(&self, label: &str, filters: Value) -> Result<Vec<Value>, GraphError> {
        let nodes = self.nodes.read().expect("nodes lock poisoned");
        Ok(nodes
            .iter()
            .filter(|(_, n)| n.labels.iter().any(|l| l == label))
            .filter(|(_, n)| props_match(&n.properties, &filters))
            .map(|(id, n)| node_to_value(id, &n.labels, &n.properties))
            .collect())
    }

    async fn get_relationships(
        &self,
        node_id: &str,
        rel_type: Option<&str>,
    ) -> Result<Vec<Value>, GraphError> {
        let edges = self.edges.read().expect("edges lock poisoned");
        Ok(edges
            .iter()
            .filter(|e| e.source_id == node_id || e.target_id == node_id)
            .filter(|e| rel_type.is_none_or(|rt| e.rel_type == rt))
            .map(edge_to_value)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn create_and_get_node() {
        let store = MemoryStore::new();
        store
            .create_node("n1", &["Person"], json!({ "name": "alice" }))
            .await
            .unwrap();
        let got = store.get_node("n1").await.unwrap().unwrap();
        assert_eq!(got["id"], "n1");
        assert_eq!(got["name"], "alice");
        assert_eq!(got["labels"], json!(["Person"]));
    }

    #[tokio::test]
    async fn missing_node_returns_none() {
        let store = MemoryStore::new();
        assert!(store.get_node("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn create_relationship_appends() {
        let store = MemoryStore::new();
        store
            .create_relationship("a", "b", "KNOWS", json!({}))
            .await
            .unwrap();
        store
            .create_relationship("a", "b", "KNOWS", json!({}))
            .await
            .unwrap();
        let rels = store.get_relationships("a", Some("KNOWS")).await.unwrap();
        assert_eq!(rels.len(), 2, "create_relationship is append, not upsert");
    }

    #[tokio::test]
    async fn merge_relationship_upserts() {
        let store = MemoryStore::new();
        store
            .merge_relationship("a", "b", "CONTAINS", json!({"v": 1}))
            .await
            .unwrap();
        store
            .merge_relationship("a", "b", "CONTAINS", json!({"v": 2}))
            .await
            .unwrap();
        let rels = store
            .get_relationships("a", Some("CONTAINS"))
            .await
            .unwrap();
        assert_eq!(rels.len(), 1, "merge_relationship must dedupe");
        assert_eq!(rels[0]["v"], 2, "merge updates properties");
    }

    #[tokio::test]
    async fn find_nodes_by_label_and_filter() {
        let store = MemoryStore::new();
        store
            .create_node("n1", &["Person"], json!({"city": "Paris"}))
            .await
            .unwrap();
        store
            .create_node("n2", &["Person"], json!({"city": "Lyon"}))
            .await
            .unwrap();
        store
            .create_node("n3", &["City"], json!({"city": "Paris"}))
            .await
            .unwrap();
        let parisians = store
            .find_nodes("Person", json!({"city": "Paris"}))
            .await
            .unwrap();
        assert_eq!(parisians.len(), 1);
        assert_eq!(parisians[0]["id"], "n1");
    }

    #[tokio::test]
    async fn get_relationships_filters_by_rel_type() {
        let store = MemoryStore::new();
        store
            .create_relationship("a", "b", "KNOWS", json!({}))
            .await
            .unwrap();
        store
            .create_relationship("a", "c", "OWNS", json!({}))
            .await
            .unwrap();
        let knows = store.get_relationships("a", Some("KNOWS")).await.unwrap();
        let all = store.get_relationships("a", None).await.unwrap();
        assert_eq!(knows.len(), 1);
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn get_relationships_returns_both_directions() {
        let store = MemoryStore::new();
        store
            .create_relationship("a", "b", "KNOWS", json!({}))
            .await
            .unwrap();
        let from_b = store.get_relationships("b", None).await.unwrap();
        assert_eq!(from_b.len(), 1, "get_relationships includes incoming");
    }

    #[tokio::test]
    async fn execute_cypher_returns_error() {
        let store = MemoryStore::new();
        let err = store
            .execute_cypher("MATCH (n) RETURN n", json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, GraphError::Backend(_)));
    }

    #[tokio::test]
    async fn counts_track_inserts() {
        let store = MemoryStore::new();
        assert_eq!(store.node_count(), 0);
        assert_eq!(store.edge_count(), 0);
        store.create_node("n1", &["X"], json!({})).await.unwrap();
        store
            .create_relationship("n1", "n2", "R", json!({}))
            .await
            .unwrap();
        assert_eq!(store.node_count(), 1);
        assert_eq!(store.edge_count(), 1);
    }
}
