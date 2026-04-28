//! HTTP server for [`polystore::GraphStore`] backends.
//!
//! Provides REST endpoints for CRUD operations, relationships, and Cypher queries.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use clap::Parser;
use graph_server::{GraphError, GraphStore, Neo4jStore};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tracing::{error, info};

/// HTTP server configuration.
#[derive(Parser, Debug)]
#[command(name = "graph-server")]
#[command(about = "HTTP server for polystore::GraphStore", long_about = None)]
struct Args {
    /// Neo4j connection URI (e.g., `bolt://localhost:7687`)
    #[arg(long, default_value = "bolt://localhost:7687")]
    neo4j_url: String,

    /// Neo4j username
    #[arg(long, default_value = "neo4j")]
    neo4j_user: String,

    /// Neo4j password
    #[arg(long, default_value = "password")]
    neo4j_pass: String,

    /// Server bind address (e.g., "0.0.0.0:50081")
    #[arg(long, default_value = "0.0.0.0:50081")]
    bind: String,
}

/// Application state holding the graph store.
struct AppState {
    store: Arc<Neo4jStore>,
}

/// Request to create a node.
#[derive(Serialize, Deserialize, Debug)]
struct CreateNodeRequest {
    id: String,
    labels: Vec<String>,
    #[serde(default)]
    properties: Value,
}

/// Request to create a relationship.
#[derive(Serialize, Deserialize, Debug)]
struct CreateEdgeRequest {
    source_id: String,
    target_id: String,
    rel_type: String,
    #[serde(default)]
    properties: Value,
}

/// Request to execute Cypher.
#[derive(Serialize, Deserialize, Debug)]
struct CypherRequest {
    query: String,
    #[serde(default)]
    params: Value,
}

/// Query parameters for search.
#[derive(Serialize, Deserialize, Debug)]
struct SearchQuery {
    q: Option<String>,
}

/// Generic JSON response wrapper.
#[derive(Serialize, Debug)]
struct JsonResponse<T: Serialize> {
    data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl<T: Serialize> JsonResponse<T> {
    fn ok(data: T) -> Self {
        Self { data, error: None }
    }
}

/// Convert [`GraphError`] to HTTP response.
fn error_response(err: &GraphError) -> (StatusCode, Json<JsonResponse<Value>>) {
    let status = match err {
        GraphError::NotFound(_) => StatusCode::NOT_FOUND,
        GraphError::Query(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error!("Graph error: {}", err);
    (
        status,
        Json(JsonResponse::ok(json!({"error": err.to_string()}))),
    )
}

/// POST /node — Create or update a node.
async fn create_node(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNodeRequest>,
) -> Result<(StatusCode, Json<JsonResponse<Value>>), (StatusCode, Json<JsonResponse<Value>>)> {
    let labels: Vec<&str> = req.labels.iter().map(String::as_str).collect();
    if let Err(e) = GraphStore::create_node(&*state.store, &req.id, &labels, req.properties).await {
        return Err(error_response(&e));
    }
    Ok((
        StatusCode::CREATED,
        Json(JsonResponse::ok(json!({"id": req.id}))),
    ))
}

/// GET /node/{id} — Get a node by ID.
async fn get_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<JsonResponse<Value>>, (StatusCode, Json<JsonResponse<Value>>)> {
    match GraphStore::get_node(&*state.store, &id).await {
        Ok(Some(node)) => Ok(Json(JsonResponse::ok(node))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(JsonResponse::ok(json!({"error": "not found"}))),
        )),
        Err(e) => Err(error_response(&e)),
    }
}

/// DELETE /node/{id} — Delete a node (stub; graph is append-only).
async fn delete_node(
    _state: State<Arc<AppState>>,
    Path(id): Path<String>,
) -> (StatusCode, Json<JsonResponse<Value>>) {
    info!(
        "Delete requested for node {} (append-only graph, no-op)",
        id
    );
    (
        StatusCode::OK,
        Json(JsonResponse::ok(
            json!({"message": "append-only graph, delete not supported"}),
        )),
    )
}

/// POST /edge — Create a relationship.
async fn create_edge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateEdgeRequest>,
) -> Result<(StatusCode, Json<JsonResponse<Value>>), (StatusCode, Json<JsonResponse<Value>>)> {
    if let Err(e) = GraphStore::create_relationship(
        &*state.store,
        &req.source_id,
        &req.target_id,
        &req.rel_type,
        req.properties,
    )
    .await
    {
        return Err(error_response(&e));
    }
    Ok((
        StatusCode::CREATED,
        Json(JsonResponse::ok(json!({
            "source": req.source_id,
            "target": req.target_id,
            "type": req.rel_type,
        }))),
    ))
}

/// GET /neighbors/{id} — Get neighbors of a node.
async fn neighbors(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<JsonResponse<Value>>, (StatusCode, Json<JsonResponse<Value>>)> {
    match GraphStore::get_relationships(&*state.store, &id, None).await {
        Ok(rels) => Ok(Json(JsonResponse::ok(json!(rels)))),
        Err(e) => Err(error_response(&e)),
    }
}

/// GET /search — Search nodes by name.
async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<JsonResponse<Value>>, (StatusCode, Json<JsonResponse<Value>>)> {
    let q = params.q.unwrap_or_default();
    let filters = json!({"name": q});

    match GraphStore::find_nodes(&*state.store, "ENTITY", filters).await {
        Ok(nodes) => Ok(Json(JsonResponse::ok(json!(nodes)))),
        Err(e) => Err(error_response(&e)),
    }
}

/// POST /query/cypher — Execute raw Cypher (admin only in production).
async fn cypher_query(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CypherRequest>,
) -> Result<Json<JsonResponse<Value>>, (StatusCode, Json<JsonResponse<Value>>)> {
    match GraphStore::execute_cypher(&*state.store, &req.query, req.params).await {
        Ok(rows) => Ok(Json(JsonResponse::ok(json!(rows)))),
        Err(e) => Err(error_response(&e)),
    }
}

/// Health check endpoint.
async fn health() -> Json<JsonResponse<Value>> {
    Json(JsonResponse::ok(json!({"status": "ok"})))
}

#[tokio::main]
async fn main() {
    // Initialize tracing.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = Args::parse();

    info!(
        "Connecting to Neo4j at {} as {}",
        args.neo4j_url, args.neo4j_user
    );

    let store = match Neo4jStore::connect(&args.neo4j_url, &args.neo4j_user, &args.neo4j_pass) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            error!("Failed to connect to Neo4j: {}", e);
            std::process::exit(1);
        }
    };

    let state = Arc::new(AppState { store });

    // Build router.
    let app = Router::new()
        .route("/health", get(health))
        .route("/node", post(create_node))
        .route("/node/:id", get(get_node).delete(delete_node))
        .route("/edge", post(create_edge))
        .route("/neighbors/:id", get(neighbors))
        .route("/search", get(search))
        .route("/query/cypher", post(cypher_query))
        .with_state(state);

    // Start server.
    let listener = match tokio::net::TcpListener::bind(&args.bind).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind to {}: {}", args.bind, e);
            std::process::exit(1);
        }
    };

    info!("Graph server listening on {}", args.bind);

    if let Err(e) = axum::serve(listener, app).await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }
}
