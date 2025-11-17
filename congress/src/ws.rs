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

async fn handle_socket(socket: WebSocket, params: WsQuery, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    let role = match params.role.as_deref() {
        Some("host") => Role::Host,
        Some("beamer") => Role::Beamer,
        Some("player") => Role::Player,
        _ => Role::Audience,
    };

    tracing::info!("WebSocket connected with role: {:?}", role);

    let game = state.get_game().await.unwrap_or_else(|| {
        // If no game exists, create one
        futures::executor::block_on(state.create_game())
    });

    let welcome = ServerMessage::Welcome {
        protocol: "1.0".to_string(),
        role: role.clone(),
        game,
    };

    if let Ok(msg) = serde_json::to_string(&welcome) {
        if sender.send(Message::Text(msg.into())).await.is_err() {
            tracing::error!("Failed to send welcome message");
            return;
        }
    }

    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                tracing::debug!("Received message: {}", text);

                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(client_msg) => {
                        if let Some(response) =
                            handle_client_message(client_msg, &role, &state).await
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
            Ok(Message::Close(_)) => {
                tracing::info!("WebSocket closed");
                break;
            }
            Ok(Message::Ping(data)) => {
                if sender.send(Message::Pong(data)).await.is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
        }
    }

    tracing::info!("WebSocket connection closed for role: {:?}", role);
}

async fn handle_client_message(
    msg: ClientMessage,
    _role: &Role,
    _state: &Arc<AppState>,
) -> Option<ServerMessage> {
    match msg {
        ClientMessage::Join { room_token } => {
            tracing::info!("Join request with token: {}", room_token);
            // just ack for now
            None
        }
        ClientMessage::SubmitAnswer { text } => {
            tracing::info!("Answer submitted: {}", text);
            // TODO: Handle submission
            None
        }
        ClientMessage::Vote { ai, funny, msg_id } => {
            tracing::info!("Vote: AI={}, Funny={}, MsgID={}", ai, funny, msg_id);
            Some(ServerMessage::VoteAck { msg_id })
        }
        ClientMessage::AckNeeded {
            last_seen_server_seq,
        } => {
            tracing::info!("Ack needed: seq={}", last_seen_server_seq);
            None
        }
    }
}
