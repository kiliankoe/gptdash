pub mod audience;
pub mod handlers;
pub mod host;
pub mod player;

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

use crate::protocol::{
    AudienceVoteInfo, ClientMessage, HostSubmissionInfo, ServerMessage, SubmissionInfo,
};
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
    let valid_transitions = AppState::get_valid_transitions(&game.phase);
    let welcome = ServerMessage::Welcome {
        protocol: "1.0".to_string(),
        role: role.clone(),
        game,
        server_now: chrono::Utc::now().to_rfc3339(),
        valid_transitions,
    };

    if let Ok(msg) = serde_json::to_string(&welcome) {
        if sender.send(Message::Text(msg.into())).await.is_err() {
            tracing::error!("Failed to send welcome message");
            return;
        }
    }

    // Send state recovery message for players/audience with tokens
    if let Some(token) = &params.token {
        match role {
            Role::Player => {
                // Try to recover player state
                if let Some(player) = state.get_player_by_token(token).await {
                    let submission = state
                        .get_player_submission_for_current_round(&player.id)
                        .await;
                    // Include current prompt for state recovery during WRITING phase
                    let current_prompt = state
                        .get_current_round()
                        .await
                        .and_then(|r| r.selected_prompt);
                    let player_state = ServerMessage::PlayerState {
                        player_id: player.id,
                        display_name: player.display_name,
                        has_submitted: submission.is_some(),
                        current_submission: submission.map(|s| SubmissionInfo::from(&s)),
                        current_prompt,
                    };
                    if let Ok(msg) = serde_json::to_string(&player_state) {
                        let _ = sender.send(Message::Text(msg.into())).await;
                    }
                    tracing::info!("Sent player state recovery for token");
                }
            }
            Role::Audience => {
                // Try to recover audience vote state
                let vote = state.get_audience_vote_for_current_round(token).await;
                let audience_state = ServerMessage::AudienceState {
                    has_voted: vote.is_some(),
                    current_vote: vote.map(|v| AudienceVoteInfo {
                        ai_pick: v.ai_pick_submission_id,
                        funny_pick: v.funny_pick_submission_id,
                    }),
                };
                if let Ok(msg) = serde_json::to_string(&audience_state) {
                    let _ = sender.send(Message::Text(msg.into())).await;
                }
                tracing::info!("Sent audience state recovery for token");
            }
            _ => {}
        }
    }

    // Send host-specific state recovery
    if role == Role::Host {
        // Send current prompts pool (independent of rounds)
        let prompts = state.get_prompts_for_host().await;
        let host_prompts = ServerMessage::HostPrompts { prompts };
        if let Ok(msg) = serde_json::to_string(&host_prompts) {
            let _ = sender.send(Message::Text(msg.into())).await;
        }

        // Send current submissions list if there's an active round
        if let Some(round) = state.get_current_round().await {
            let submissions = state.get_submissions(&round.id).await;
            let host_submissions = ServerMessage::HostSubmissions {
                list: submissions.iter().map(HostSubmissionInfo::from).collect(),
            };
            if let Ok(msg) = serde_json::to_string(&host_submissions) {
                let _ = sender.send(Message::Text(msg.into())).await;
            }
        }

        // Send current player status
        let player_status = state.get_all_player_status().await;
        let status_msg = ServerMessage::HostPlayerStatus {
            players: player_status,
        };
        if let Ok(msg) = serde_json::to_string(&status_msg) {
            let _ = sender.send(Message::Text(msg.into())).await;
        }

        tracing::info!("Sent host state recovery");
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
