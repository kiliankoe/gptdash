pub mod handlers;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use std::sync::Arc;

use crate::protocol::{ClientMessage, ServerMessage};
use crate::state::AppState;
use crate::types::Role;

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub role: Option<String>,
    pub token: Option<String>,
}

/// WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    tracing::info!(
        "WebSocket connection request: role={:?}, token={:?}",
        params.role,
        params.token
    );

    ws.on_upgrade(move |socket| handle_socket(socket, params, state))
}

/// Handle individual WebSocket connection
async fn handle_socket(socket: WebSocket, params: WsQuery, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    let role = match params.role.as_deref() {
        Some("host") => Role::Host,
        Some("beamer") => Role::Beamer,
        Some("player") => Role::Player,
        _ => Role::Audience,
    };

    tracing::info!("WebSocket connected with role: {:?}", role);

    // Ensure a game exists
    let game = match state.get_game().await {
        Some(g) => g,
        None => {
            tracing::warn!("No game found, creating one");
            state.create_game().await
        }
    };

    // Send welcome message
    let welcome = ServerMessage::Welcome {
        protocol: "1.0".to_string(),
        role: role.clone(),
        game,
        server_now: chrono::Utc::now().to_rfc3339(),
    };

    if let Ok(msg) = serde_json::to_string(&welcome) {
        if sender.send(Message::Text(msg.into())).await.is_err() {
            tracing::error!("Failed to send welcome message");
            return;
        }
    }

    // Subscribe to general broadcast (all clients)
    let mut broadcast_rx = state.broadcast.subscribe();

    // Subscribe to Host-specific broadcast if Host
    let mut host_broadcast_rx = if role == Role::Host {
        Some(state.host_broadcast.subscribe())
    } else {
        None
    };

    // Subscribe to Beamer-specific broadcast if Beamer
    let mut beamer_broadcast_rx = if role == Role::Beamer {
        Some(state.beamer_broadcast.subscribe())
    } else {
        None
    };

    // Handle incoming messages and broadcasts
    loop {
        tokio::select! {
            // Handle general broadcasts (all clients)
            broadcast_msg = broadcast_rx.recv() => {
                if let Ok(msg) = broadcast_msg {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }

            // Handle Host-specific broadcasts
            host_msg = async {
                match &mut host_broadcast_rx {
                    Some(rx) => rx.recv().await.ok(),
                    None => {
                        // Non-Host: wait forever
                        std::future::pending::<Option<ServerMessage>>().await
                    }
                }
            } => {
                if let Some(msg) = host_msg {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }

            // Handle Beamer-specific broadcasts
            beamer_msg = async {
                match &mut beamer_broadcast_rx {
                    Some(rx) => rx.recv().await.ok(),
                    None => {
                        // Non-Beamer: wait forever
                        std::future::pending::<Option<ServerMessage>>().await
                    }
                }
            } => {
                if let Some(msg) = beamer_msg {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }

            // Handle client messages
            ws_msg = receiver.next() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        tracing::debug!("Received message: {}", text);

                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(client_msg) => {
                                if let Some(response) =
                                    handlers::handle_message(client_msg, &role, &state).await
                                {
                                    if let Ok(json) = serde_json::to_string(&response) {
                                        if sender.send(Message::Text(json.into())).await.is_err() {
                                            tracing::error!("Failed to send response");
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Failed to parse client message: {}", e);
                                let error = ServerMessage::Error {
                                    code: "PARSE_ERROR".to_string(),
                                    msg: format!("Invalid message format: {}", e),
                                };
                                if let Ok(json) = serde_json::to_string(&error) {
                                    let _ = sender.send(Message::Text(json.into())).await;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        tracing::info!("WebSocket closed");
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::error!("WebSocket error: {}", e);
                        break;
                    }
                    None => break,
                }
            }
        }
    }

    tracing::info!("WebSocket connection closed for role: {:?}", role);
}
