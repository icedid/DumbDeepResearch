pub mod ws;

use axum::{routing::get, Router};
use ddr_engine::ResearchCoordinator;
use ddr_protocol::EngineBroadcast;
use tokio::sync::broadcast;

async fn health_check() -> &'static str {
    "ok"
}

pub fn build_router(coordinator: ResearchCoordinator) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/ws", get(ws::ws_handler))
        .with_state(coordinator)
}

#[tokio::main]
async fn main() {
    let (broadcast_tx, _) = broadcast::channel::<EngineBroadcast>(256);
    let coordinator = ResearchCoordinator::new(broadcast_tx);
    let app = build_router(coordinator);

    let bind_addr = std::env::var("DDR_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("failed to bind TCP listener");

    println!("DumbDeepResearch (DDR) Server listening on http://{bind_addr} (WS: /ws)");
    axum::serve(listener, app)
        .await
        .expect("Axum server exited with error");
}
