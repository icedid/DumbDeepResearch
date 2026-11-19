pub mod ws;

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use ddr_engine::ResearchCoordinator;
use ddr_knowledge::dossier::generate_audit_dossier;
use ddr_protocol::{AuditDossier, EngineBroadcast, ExecutionMode, KnowledgeNode};
use serde::Serialize;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Serialize)]
struct SessionConfigDto {
    max_depth: u8,
    max_sources: usize,
    mode: ExecutionMode,
}

#[derive(Serialize)]
struct GraphSnapshotResponse {
    nodes: Vec<KnowledgeNode>,
    edges: Vec<(Uuid, Uuid)>,
    config: SessionConfigDto,
}

async fn health_check() -> &'static str {
    "ok"
}

async fn get_graph_snapshot(
    State(coordinator): State<ResearchCoordinator>,
) -> Json<GraphSnapshotResponse> {
    let graph = coordinator.graph.read().await;
    let cfg = *coordinator.session_config.read().await;
    Json(GraphSnapshotResponse {
        nodes: graph.all_nodes(),
        edges: graph.all_edges(),
        config: SessionConfigDto {
            max_depth: cfg.max_depth,
            max_sources: cfg.max_sources,
            mode: cfg.mode,
        },
    })
}

async fn get_dossier(State(coordinator): State<ResearchCoordinator>) -> Json<AuditDossier> {
    let graph = coordinator.graph.read().await;
    Json(generate_audit_dossier(&graph))
}

async fn get_dossier_markdown(
    State(coordinator): State<ResearchCoordinator>,
) -> impl IntoResponse {
    let graph = coordinator.graph.read().await;
    let dossier = generate_audit_dossier(&graph);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        dossier.markdown,
    )
}

pub fn build_router(coordinator: ResearchCoordinator) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/ws", get(ws::ws_handler))
        .route("/api/graph", get(get_graph_snapshot))
        .route("/api/dossier", get(get_dossier))
        .route("/api/dossier/markdown", get(get_dossier_markdown))
        .layer(CorsLayer::permissive())
        .with_state(coordinator)
}

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,ddr_engine=debug,ddr_crawler=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let (broadcast_tx, _) = broadcast::channel::<EngineBroadcast>(256);
    let coordinator = ResearchCoordinator::new(broadcast_tx);
    let app = build_router(coordinator);

    let bind_addr =
        std::env::var("DDR_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("failed to bind TCP listener");

    println!("DumbDeepResearch (DDR) Server listening on http://{bind_addr} (WS: /ws)");
    axum::serve(listener, app)
        .await
        .expect("Axum server exited with error");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_and_api_routes_respond() {
        let (tx, _) = broadcast::channel(16);
        let coord = ResearchCoordinator::new(tx);
        let app = build_router(coord);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let base = format!("http://{addr}");
        let health = reqwest::get(format!("{base}/health"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(health, "ok");

        let graph_resp: serde_json::Value = reqwest::get(format!("{base}/api/graph"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(graph_resp["nodes"].is_array());

        let dossier_resp: AuditDossier = reqwest::get(format!("{base}/api/dossier"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(dossier_resp.citation_audit.passed);

        server_task.abort();
    }
}
