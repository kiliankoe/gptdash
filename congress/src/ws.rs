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

    // Ensure a game exists
    let game = match state.get_game().await {
        Some(g) => g,
        None => {
            tracing::warn!("No game found, creating one");
            state.create_game().await
        }
    };

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
    role: &Role,
    state: &Arc<AppState>,
) -> Option<ServerMessage> {
    match msg {
        ClientMessage::Join { room_token } => {
            tracing::info!("Join request with token: {}", room_token);
            None
        }

        ClientMessage::RegisterPlayer {
            player_token,
            display_name,
        } => {
            tracing::info!("Player registration: {}", display_name);
            match state.register_player(&player_token, display_name.clone()).await {
                Ok(player) => Some(ServerMessage::PlayerRegistered {
                    player_id: player.id,
                    display_name,
                }),
                Err(e) => Some(ServerMessage::Error {
                    code: "REGISTRATION_FAILED".to_string(),
                    msg: e,
                }),
            }
        }

        ClientMessage::SubmitAnswer { player_token, text } => {
            tracing::info!("Answer submitted: {}", text);
            if let Some(round) = state.get_current_round().await {
                // Look up player ID from token if provided
                let player_id = if let Some(token) = player_token {
                    state.get_player_by_token(&token).await.map(|p| p.id)
                } else {
                    None
                };

                match state.submit_answer(&round.id, player_id, text).await {
                    Ok(_) => None,
                    Err(e) => Some(ServerMessage::Error {
                        code: "SUBMISSION_FAILED".to_string(),
                        msg: e,
                    }),
                }
            } else {
                Some(ServerMessage::Error {
                    code: "NO_ACTIVE_ROUND".to_string(),
                    msg: "No active round".to_string(),
                })
            }
        }

        ClientMessage::Vote { voter_token, ai, funny, msg_id } => {
            tracing::info!("Vote: AI={}, Funny={}, MsgID={}", ai, funny, msg_id);
            if let Some(round) = state.get_current_round().await {
                // Store the vote
                let vote = crate::types::Vote {
                    id: ulid::Ulid::new().to_string(),
                    round_id: round.id,
                    voter_id: voter_token,
                    ai_pick_submission_id: ai,
                    funny_pick_submission_id: funny,
                    ts: chrono::Utc::now().to_rfc3339(),
                };
                state.votes.write().await.insert(vote.id.clone(), vote);
            }
            Some(ServerMessage::VoteAck { msg_id })
        }

        ClientMessage::SubmitPrompt { text } => {
            tracing::info!("Prompt submitted: {}", text);
            if let Some(round) = state.get_current_round().await {
                match state
                    .add_prompt(&round.id, text, crate::types::PromptSource::Audience)
                    .await
                {
                    Ok(_) => None,
                    Err(e) => Some(ServerMessage::Error {
                        code: "PROMPT_FAILED".to_string(),
                        msg: e,
                    }),
                }
            } else {
                Some(ServerMessage::Error {
                    code: "NO_ACTIVE_ROUND".to_string(),
                    msg: "No active round".to_string(),
                })
            }
        }

        ClientMessage::AckNeeded {
            last_seen_server_seq,
        } => {
            tracing::info!("Ack needed: seq={}", last_seen_server_seq);
            None
        }

        // Host-only commands
        ClientMessage::HostCreatePlayers { count } => {
            if *role != Role::Host {
                return Some(ServerMessage::Error {
                    code: "UNAUTHORIZED".to_string(),
                    msg: "Only host can create players".to_string(),
                });
            }

            tracing::info!("Host creating {} players", count);
            let mut players = Vec::new();
            for _ in 0..count {
                let player = state.create_player().await;
                players.push(crate::protocol::PlayerToken {
                    id: player.id,
                    token: player.token,
                });
            }

            Some(ServerMessage::PlayersCreated { players })
        }

        ClientMessage::HostTransitionPhase { phase } => {
            if *role != Role::Host {
                return Some(ServerMessage::Error {
                    code: "UNAUTHORIZED".to_string(),
                    msg: "Only host can transition phases".to_string(),
                });
            }

            tracing::info!("Host transitioning to phase: {:?}", phase);
            match state.transition_phase(phase).await {
                Ok(_) => {
                    if let Some(game) = state.get_game().await {
                        Some(ServerMessage::GameState { game })
                    } else {
                        None
                    }
                }
                Err(e) => Some(ServerMessage::Error {
                    code: "TRANSITION_FAILED".to_string(),
                    msg: e,
                }),
            }
        }

        ClientMessage::HostStartRound => {
            if *role != Role::Host {
                return Some(ServerMessage::Error {
                    code: "UNAUTHORIZED".to_string(),
                    msg: "Only host can start rounds".to_string(),
                });
            }

            tracing::info!("Host starting new round");
            match state.start_round().await {
                Ok(round) => Some(ServerMessage::RoundStarted { round }),
                Err(e) => Some(ServerMessage::Error {
                    code: "ROUND_START_FAILED".to_string(),
                    msg: e,
                }),
            }
        }

        ClientMessage::HostSelectPrompt { prompt_id } => {
            if *role != Role::Host {
                return Some(ServerMessage::Error {
                    code: "UNAUTHORIZED".to_string(),
                    msg: "Only host can select prompts".to_string(),
                });
            }

            tracing::info!("Host selecting prompt: {}", prompt_id);
            if let Some(round) = state.get_current_round().await {
                match state.select_prompt(&round.id, &prompt_id).await {
                    Ok(_) => {
                        if let Some(updated_round) = state.get_current_round().await {
                            if let Some(prompt) = updated_round.selected_prompt {
                                Some(ServerMessage::PromptSelected { prompt })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    Err(e) => Some(ServerMessage::Error {
                        code: "PROMPT_SELECT_FAILED".to_string(),
                        msg: e,
                    }),
                }
            } else {
                Some(ServerMessage::Error {
                    code: "NO_ACTIVE_ROUND".to_string(),
                    msg: "No active round".to_string(),
                })
            }
        }

        ClientMessage::HostEditSubmission {
            submission_id,
            new_text,
        } => {
            if *role != Role::Host {
                return Some(ServerMessage::Error {
                    code: "UNAUTHORIZED".to_string(),
                    msg: "Only host can edit submissions".to_string(),
                });
            }

            tracing::info!("Host editing submission: {}", submission_id);
            match state.edit_submission(&submission_id, new_text).await {
                Ok(_) => None,
                Err(e) => Some(ServerMessage::Error {
                    code: "EDIT_FAILED".to_string(),
                    msg: e,
                }),
            }
        }

        ClientMessage::HostSetRevealOrder { order } => {
            if *role != Role::Host {
                return Some(ServerMessage::Error {
                    code: "UNAUTHORIZED".to_string(),
                    msg: "Only host can set reveal order".to_string(),
                });
            }

            tracing::info!("Host setting reveal order: {} items", order.len());
            if let Some(round) = state.get_current_round().await {
                match state.set_reveal_order(&round.id, order).await {
                    Ok(_) => None,
                    Err(e) => Some(ServerMessage::Error {
                        code: "REVEAL_ORDER_FAILED".to_string(),
                        msg: e,
                    }),
                }
            } else {
                Some(ServerMessage::Error {
                    code: "NO_ACTIVE_ROUND".to_string(),
                    msg: "No active round".to_string(),
                })
            }
        }
    }
}
