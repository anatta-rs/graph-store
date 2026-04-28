# polystore-neo4j

Neo4j Bolt adapter for polystore::GraphStore — extracted from [Anatta](https://github.com/anatta-rs/Anatta).

## Status

v0.1.0 — initial extract from Anatta codebase. API may evolve as this crate stabilizes.

## Overview

This crate provides:
- **`GraphStore` trait**: abstract async graph interface for CRUD + Cypher queries
- **`Neo4jStore` impl**: Bolt protocol adapter backed by neo4rs

All operations are append-only; entities and relationships are immutable once created.

## Usage

```rust
use polystore_neo4j::Neo4jStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = Neo4jStore::connect(
        "bolt://localhost:7687",
        "neo4j",
        "password"
    )?;

    let result = store.execute_cypher(
        "MATCH (n) RETURN n LIMIT 10",
        serde_json::json!({})
    ).await?;

    println!("{:?}", result);
    Ok(())
}
```

## Features

- Raw Cypher execution with typed parameter support
- Node CRUD: create, get, find
- Relationship operations: create, merge (idempotent), query
- Full Bolt type conversions (scalars, lists, maps, nodes)
- Async/await with tokio
- Comprehensive error handling

## Dependencies

- **neo4rs** (0.9.0-rc.9): Bolt protocol client
- **serde/serde_json**: JSON serialization
- **tokio**: async runtime
- **chrono**: timestamp handling
- **thiserror**: error types

## Reference

For the polystore trait abstraction, see [anatta-rs/polystore](https://github.com/anatta-rs/polystore).

## License

MIT — see LICENSE file.
