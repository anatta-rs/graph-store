//! `Neo4j` implementation of [`GraphStore`].

use neo4rs::{
    BoltBoolean, BoltFloat, BoltInteger, BoltList, BoltMap, BoltString, BoltType, Graph, query,
};

use crate::error::GraphError;
use crate::store::GraphStore;

/// Helper for collecting typed params before applying to a query.
enum ParamValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// [`GraphStore`] backed by a `Neo4j` instance via Bolt protocol.
pub struct Neo4jStore {
    graph: Graph,
}

impl Neo4jStore {
    /// Connect to a `Neo4j` instance.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::Backend`] if the connection fails.
    pub fn connect(uri: &str, user: &str, password: &str) -> Result<Self, GraphError> {
        let graph = Graph::new(uri, user, password)?;
        Ok(Self { graph })
    }
}

impl GraphStore for Neo4jStore {
    async fn execute_cypher(
        &self,
        cypher: &str,
        params: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, GraphError> {
        let mut q = query(cypher);

        if let serde_json::Value::Object(map) = params {
            for (key, value) in map {
                q = q.param(&key, json_value_to_bolt(&value));
            }
        }

        let mut result = self.graph.execute(q).await?;
        let mut rows = Vec::new();

        while let Some(row) = result.next().await? {
            let mut obj = serde_json::Map::new();
            for bolt_key in row.keys() {
                let key = &bolt_key.value;
                if let Ok(val) = row.get::<BoltType>(key) {
                    obj.insert(key.clone(), bolt_to_json(&val));
                }
            }
            rows.push(serde_json::Value::Object(obj));
        }

        Ok(rows)
    }

    async fn create_node(
        &self,
        id: &str,
        labels: &[&str],
        properties: serde_json::Value,
    ) -> Result<(), GraphError> {
        let label_str = labels.join(":");
        let mut set_clauses = Vec::new();
        // Collect params as (name, value) to apply in one pass.
        let mut params: Vec<(String, ParamValue)> = Vec::new();

        if let serde_json::Value::Object(map) = properties {
            for (key, value) in map {
                let param_name = format!("p_{key}");
                set_clauses.push(format!("n.{key} = ${param_name}"));
                match value {
                    serde_json::Value::String(s) => {
                        params.push((param_name, ParamValue::Str(s)));
                    }
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            params.push((param_name, ParamValue::Int(i)));
                        } else if let Some(f) = n.as_f64() {
                            params.push((param_name, ParamValue::Float(f)));
                        }
                    }
                    serde_json::Value::Bool(b) => {
                        params.push((param_name, ParamValue::Bool(b)));
                    }
                    _ => {}
                }
            }
        }

        let set_str = if set_clauses.is_empty() {
            String::new()
        } else {
            format!(" SET {}", set_clauses.join(", "))
        };

        let cypher = format!("CREATE (n:{label_str} {{id: $id}}){set_str}");
        let mut q = query(&cypher).param("id", id.to_string());

        for (name, value) in params {
            q = match value {
                ParamValue::Str(s) => q.param(&name, s),
                ParamValue::Int(i) => q.param(&name, i),
                ParamValue::Float(f) => q.param(&name, f),
                ParamValue::Bool(b) => q.param(&name, b),
            };
        }

        self.graph.run(q).await?;
        Ok(())
    }

    async fn get_node(&self, id: &str) -> Result<Option<serde_json::Value>, GraphError> {
        // Return all node properties (including content_hash, kind, source_id,
        // identity_key, etc.) — callers depend on them (e.g. engine.get_content
        // reads content_hash to resolve via KV).
        let q =
            query("MATCH (n {id: $id}) RETURN properties(n) AS props").param("id", id.to_string());

        let mut result = self.graph.execute(q).await?;

        if let Some(row) = result.next().await?
            && let Ok(props) = row.get::<neo4rs::BoltMap>("props")
        {
            let mut obj = serde_json::Map::new();
            for (k, v) in &props.value {
                let key = k.value.clone();
                let val = bolt_to_json(v);
                obj.insert(key, val);
            }
            return Ok(Some(serde_json::Value::Object(obj)));
        }

        Ok(None)
    }

    async fn find_nodes(
        &self,
        label: &str,
        filters: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, GraphError> {
        let mut conditions = Vec::new();
        let mut q_params: Vec<(String, String)> = Vec::new();

        if let serde_json::Value::Object(map) = &filters {
            for (key, value) in map {
                let param_name = format!("f_{key}");
                conditions.push(format!("n.{key} = ${param_name}"));
                if let serde_json::Value::String(s) = value {
                    q_params.push((param_name, s.clone()));
                }
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        let cypher = format!(
            "MATCH (n:{label}){where_clause} RETURN n.id AS id, n.name AS name, \
             n.entity_type AS entity_type, n.description AS description"
        );

        let mut q = query(&cypher);
        for (param_name, param_value) in q_params {
            q = q.param(&param_name, param_value);
        }

        let mut result = self.graph.execute(q).await?;
        let mut rows = Vec::new();

        while let Some(row) = result.next().await? {
            let mut obj = serde_json::Map::new();
            for bolt_key in row.keys() {
                let key = &bolt_key.value;
                if let Ok(val) = row.get::<String>(key) {
                    obj.insert(key.clone(), serde_json::Value::String(val));
                }
            }
            rows.push(serde_json::Value::Object(obj));
        }

        Ok(rows)
    }

    async fn create_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
        properties: serde_json::Value,
    ) -> Result<(), GraphError> {
        let mut set_clauses = Vec::new();
        let mut params: Vec<(String, ParamValue)> = Vec::new();

        // Auto-add created_at if not provided.
        let mut props = match properties {
            serde_json::Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        if !props.contains_key("created_at") {
            props.insert(
                "created_at".to_string(),
                serde_json::Value::String(chrono::Utc::now().to_rfc3339()),
            );
        }

        for (key, value) in props {
            let param_name = format!("p_{key}");
            set_clauses.push(format!("r.{key} = ${param_name}"));
            match value {
                serde_json::Value::String(s) => {
                    params.push((param_name, ParamValue::Str(s)));
                }
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        params.push((param_name, ParamValue::Int(i)));
                    } else if let Some(f) = n.as_f64() {
                        params.push((param_name, ParamValue::Float(f)));
                    }
                }
                serde_json::Value::Bool(b) => {
                    params.push((param_name, ParamValue::Bool(b)));
                }
                _ => {}
            }
        }

        let set_str = if set_clauses.is_empty() {
            String::new()
        } else {
            format!(" SET {}", set_clauses.join(", "))
        };

        let cypher = format!(
            "MATCH (a {{id: $source_id}}), (b {{id: $target_id}}) \
             CREATE (a)-[r:{rel_type}]->(b){set_str} RETURN type(r)"
        );

        let mut q = query(&cypher)
            .param("source_id", source_id.to_string())
            .param("target_id", target_id.to_string());

        for (name, value) in params {
            q = match value {
                ParamValue::Str(s) => q.param(&name, s),
                ParamValue::Int(i) => q.param(&name, i),
                ParamValue::Float(f) => q.param(&name, f),
                ParamValue::Bool(b) => q.param(&name, b),
            };
        }

        self.graph.run(q).await?;
        Ok(())
    }

    async fn merge_relationship(
        &self,
        source_id: &str,
        target_id: &str,
        rel_type: &str,
        properties: serde_json::Value,
    ) -> Result<(), GraphError> {
        let mut set_clauses = Vec::new();
        let mut params: Vec<(String, ParamValue)> = Vec::new();

        let mut props = match properties {
            serde_json::Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        // Auto-add created_at if absent. MERGE + SET leaves created_at at the
        // most recent write; not ideal for historical edges, but CONTAINS/NEXT
        // are structural (no event semantics) so latest-writer-wins is fine.
        if !props.contains_key("created_at") {
            props.insert(
                "created_at".to_string(),
                serde_json::Value::String(chrono::Utc::now().to_rfc3339()),
            );
        }

        for (key, value) in props {
            let param_name = format!("p_{key}");
            set_clauses.push(format!("r.{key} = ${param_name}"));
            match value {
                serde_json::Value::String(s) => {
                    params.push((param_name, ParamValue::Str(s)));
                }
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        params.push((param_name, ParamValue::Int(i)));
                    } else if let Some(f) = n.as_f64() {
                        params.push((param_name, ParamValue::Float(f)));
                    }
                }
                serde_json::Value::Bool(b) => {
                    params.push((param_name, ParamValue::Bool(b)));
                }
                _ => {}
            }
        }

        let set_str = if set_clauses.is_empty() {
            String::new()
        } else {
            format!(" SET {}", set_clauses.join(", "))
        };

        // MERGE is idempotent on (source_id, target_id, rel_type). Calling
        // this repeatedly with the same endpoints leaves exactly one edge.
        let cypher = format!(
            "MATCH (a {{id: $source_id}}), (b {{id: $target_id}}) \
             MERGE (a)-[r:{rel_type}]->(b){set_str} RETURN type(r)"
        );

        let mut q = query(&cypher)
            .param("source_id", source_id.to_string())
            .param("target_id", target_id.to_string());

        for (name, value) in params {
            q = match value {
                ParamValue::Str(s) => q.param(&name, s),
                ParamValue::Int(i) => q.param(&name, i),
                ParamValue::Float(f) => q.param(&name, f),
                ParamValue::Bool(b) => q.param(&name, b),
            };
        }

        self.graph.run(q).await?;
        Ok(())
    }

    async fn get_relationships(
        &self,
        node_id: &str,
        rel_type: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, GraphError> {
        let rel_filter = rel_type.map_or_else(String::new, |t| format!(":{t}"));
        let cypher = format!(
            "MATCH (n {{id: $id}})-[r{rel_filter}]-(m) \
             RETURN type(r) AS rel_type, m.id AS target_id, m.name AS target_name"
        );

        let q = query(&cypher).param("id", node_id.to_string());
        let mut result = self.graph.execute(q).await?;
        let mut rows = Vec::new();

        while let Some(row) = result.next().await? {
            let mut obj = serde_json::Map::new();
            for bolt_key in row.keys() {
                let key = &bolt_key.value;
                if let Ok(val) = row.get::<String>(key) {
                    obj.insert(key.clone(), serde_json::Value::String(val));
                }
            }
            rows.push(serde_json::Value::Object(obj));
        }

        Ok(rows)
    }
}

/// Convert a `BoltType` returned by a Cypher query into `serde_json::Value`.
///
/// Previously `execute_cypher` only handled scalar types + Vec<String>/Vec<f64>,
/// silently dropping lists of maps (e.g. `collect({id, name})`). This recursive
/// converter covers the full Bolt value space used by Cypher.
fn bolt_to_json(v: &BoltType) -> serde_json::Value {
    match v {
        BoltType::Null(_) => serde_json::Value::Null,
        BoltType::String(s) => serde_json::Value::String(s.value.clone()),
        BoltType::Integer(i) => serde_json::Value::Number(serde_json::Number::from(i.value)),
        BoltType::Float(f) => serde_json::Number::from_f64(f.value)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        BoltType::Boolean(b) => serde_json::Value::Bool(b.value),
        BoltType::List(list) => {
            serde_json::Value::Array(list.value.iter().map(bolt_to_json).collect())
        }
        BoltType::Map(map) => {
            let mut obj = serde_json::Map::new();
            for (k, val) in &map.value {
                obj.insert(k.value.clone(), bolt_to_json(val));
            }
            serde_json::Value::Object(obj)
        }
        BoltType::Node(n) => {
            let mut obj = serde_json::Map::new();
            for (k, val) in &n.properties.value {
                obj.insert(k.value.clone(), bolt_to_json(val));
            }
            serde_json::Value::Object(obj)
        }
        // Fallback: render unsupported Bolt types (dates, points, relations) as debug strings.
        other => serde_json::Value::String(format!("{other:?}")),
    }
}

/// Convert a JSON value to `BoltType` (recursive, handles all types).
fn json_value_to_bolt(v: &serde_json::Value) -> BoltType {
    match v {
        serde_json::Value::String(s) => BoltType::String(BoltString::from(s.as_str())),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                BoltType::Integer(BoltInteger::new(i))
            } else if let Some(f) = n.as_f64() {
                BoltType::Float(BoltFloat::new(f))
            } else {
                BoltType::Null(neo4rs::BoltNull)
            }
        }
        serde_json::Value::Bool(b) => BoltType::Boolean(BoltBoolean::new(*b)),
        serde_json::Value::Null => BoltType::Null(neo4rs::BoltNull),
        serde_json::Value::Array(arr) => {
            let items: Vec<BoltType> = arr.iter().map(json_value_to_bolt).collect();
            BoltType::List(BoltList::from(items))
        }
        serde_json::Value::Object(obj) => {
            let mut map = BoltMap::new();
            for (k, val) in obj {
                map.put(BoltString::from(k.as_str()), json_value_to_bolt(val));
            }
            BoltType::Map(map)
        }
    }
}
