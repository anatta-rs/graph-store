# graph-store

> Implementations of [`polystore::GraphStore`](https://github.com/anatta-rs/polystore) for multiple backends, plus an optional HTTP server bin.

## Philosophy

- **Lib-first**: `GraphStore` trait impls for multiple backends, each gated by a feature flag. Embed directly into your application.
- **Bin-optional**: an HTTP server (`graph-server` bin) for distributed access. Only compiled when a backend feature is enabled.
- **Backends as features**: pick one or many; runtime CLI flag selects which backend the bin uses.

## Backends

| Feature | Crate dep | Status |
|---|---|---|
| `neo4j-backend` (default) | `neo4rs` (Bolt protocol) | ready |
| `memory-backend` | — | placeholder |

## Use as a library

```rust
use graph_store::{GraphStore, Neo4jStore};

let store = Neo4jStore::connect(
    "bolt://localhost:7687",
    "neo4j",
    "password",
)?;

let rows = store.execute_cypher(
    "MATCH (n) RETURN n LIMIT 10",
    serde_json::json!({}),
).await?;
```

Disable defaults to ship without Neo4j :

```toml
[dependencies]
graph-store = { git = "https://github.com/anatta-rs/graph-store", default-features = false, features = ["memory-backend"] }
```

## Use as a binary (HTTP server)

```bash
cargo install --git https://github.com/anatta-rs/graph-store --features neo4j-backend
NEO4J_URL=bolt://localhost:7687 NEO4J_USER=neo4j NEO4J_PASS=… graph-server
```

CLI flags pick the backend at runtime when multiple are compiled in.

## HTTP API

```
GET    /health                → 200 OK
POST   /node                  → upsert node {id, kind, payload}
GET    /node/{id}             → 200 + JSON node | 404
DELETE /node/{id}             → 204
POST   /edge                  → add edge {from, to, kind, role}
GET    /neighbors/{id}        → 200 + JSON list of neighbors
GET    /search?q=…            → 200 + JSON hits
POST   /query/cypher          → 200 + JSON rows (escape hatch, admin)
```

## Roadmap

| | |
|---|---|
| ✅ neo4j-backend | |
| ✅ HTTP server bin | |
| 🟡 memory-backend (in-memory impl) | |
| 🟡 cypher escape hatch behind admin auth | |

## License

Apache-2.0.
