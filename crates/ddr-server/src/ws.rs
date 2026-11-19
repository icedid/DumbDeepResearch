use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use ddr_engine::ResearchCoordinator;
use ddr_protocol::{ClientAction, EngineBroadcast};
use tokio::sync::broadcast;
use tracing::{debug, error, warn};

/// Axum WebSocket upgrade handler (`/ws`) bridging the React Flow Canvas and `ddr-engine`.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(coordinator): State<ResearchCoordinator>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, coordinator))
}

async fn handle_socket(mut socket: WebSocket, coordinator: ResearchCoordinator) {
    let mut broadcast_rx = coordinator.broadcast_tx.subscribe();
    debug!("WebSocket client connected");

    // Send initial GraphSnapshot to synchronize newly connected canvas clients.
    let initial_snapshot = {
        let graph = coordinator.graph.read().await;
        let nodes = graph.all_nodes();
        let cfg = *coordinator.session_config.read().await;
        EngineBroadcast::GraphSnapshot {
            nodes,
            max_depth: cfg.max_depth,
            max_sources: cfg.max_sources,
            mode: cfg.mode,
        }
    };

    if let Ok(json) = serde_json::to_string(&initial_snapshot) {
        if socket.send(Message::Text(json)).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientAction>(&text) {
                            Ok(action) => {
                                coordinator.handle_action(action).await;
                            }
                            Err(err) => {
                                warn!(%err, payload = %text, "Invalid ClientAction JSON received");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        debug!("WebSocket client sent Close frame");
                        let _ = socket.close().await;
                        return;
                    }
                    None => {
                        debug!("WebSocket client disconnected");
                        return;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        debug!(%err, "WebSocket connection closed");
                        return;
                    }
                }
            }
            event = broadcast_rx.recv() => {
                match event {
                    Ok(broadcast) => {
                        match serde_json::to_string(&broadcast) {
                            Ok(json) => {
                                if socket.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                            Err(err) => {
                                error!(%err, "Failed to serialize EngineBroadcast");
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "WebSocket client lagged behind broadcast channel");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        }
    }
}
