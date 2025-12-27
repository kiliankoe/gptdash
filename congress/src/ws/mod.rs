pub mod audience;
pub mod handlers;
pub mod host;
pub mod player;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use crate::protocol::{
    AudienceVoteInfo, ClientMessage, HostSubmissionInfo, ServerMessage, SubmissionInfo,
};
use crate::state::AppState;
use crate::types::{GamePhase, Role};
use futures::stream::SplitSink;
use serde::Serialize;

const MAX_WS_MESSAGE_BYTES: usize = 32 * 1024;

/// Helper to serialize and send a message over WebSocket.
/// Returns Ok(true) if sent successfully, Ok(false) if serialization failed (logged), Err if send failed.
async fn send_json<T: Serialize>(
    sender: &mut SplitSink<WebSocket, Message>,
    msg: &T,
) -> Result<bool, ()> {
    match serde_json::to_string(msg) {
        Ok(json) => {
            if sender.send(Message::Text(json.into())).await.is_err() {
                Err(())
            } else {
                Ok(true)
            }
        }
        Err(e) => {
            tracing::error!("Failed to serialize message: {}", e);
            Ok(false)
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub role: Option<String>,
    pub token: Option<String>,
}

fn token_required_for_role(role: &Role) -> bool {
    matches!(role, Role::Player | Role::Audience)
}

/// Extract client IP from request, considering X-Forwarded-For for reverse proxy
fn extract_client_ip(headers: &HeaderMap, connect_info: &SocketAddr) -> IpAddr {
    if let Some(xff) = headers.get("x-forwarded-for") {
        if let Ok(xff_str) = xff.to_str() {
            if let Some(first_ip) = xff_str.split(',').next() {
                if let Ok(ip) = first_ip.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }
    connect_info.ip()
}

fn validate_message_for_role(
    role: &Role,
    connection_token: Option<&str>,
    msg: &ClientMessage,
) -> Result<(), ValidationError> {
    // We allow Join/AckNeeded from anyone (including beamer/host), but everything else is role-bound.
    match role {
        Role::Audience => match msg {
            ClientMessage::Join { .. } | ClientMessage::AckNeeded { .. } => Ok(()),
            ClientMessage::Vote { voter_token, .. }
            | ClientMessage::PromptVote { voter_token, .. }
            | ClientMessage::SubmitTriviaVote { voter_token, .. } => {
                if connection_token == Some(voter_token.as_str()) {
                    Ok(())
                } else {
                    Err(ValidationError::TokenMismatch)
                }
            }
            ClientMessage::SubmitPrompt { voter_token, .. } => {
                let Some(voter_token) = voter_token.as_deref() else {
                    return Err(ValidationError::TokenRequired("Missing voter token"));
                };
                if connection_token == Some(voter_token) {
                    Ok(())
                } else {
                    Err(ValidationError::TokenMismatch)
                }
            }
            _ => Err(ValidationError::UnauthorizedRole(
                "Message type not allowed for audience connections",
            )),
        },
        Role::Player => match msg {
            ClientMessage::Join { .. } | ClientMessage::AckNeeded { .. } => Ok(()),
            ClientMessage::RegisterPlayer { player_token, .. }
            | ClientMessage::RequestTypoCheck { player_token, .. }
            | ClientMessage::UpdateSubmission { player_token, .. } => {
                if connection_token == Some(player_token.as_str()) {
                    Ok(())
                } else {
                    Err(ValidationError::TokenMismatch)
                }
            }
            ClientMessage::SubmitAnswer { player_token, .. } => {
                let Some(player_token) = player_token.as_deref() else {
                    return Err(ValidationError::TokenRequired("Missing player token"));
                };
                if connection_token == Some(player_token) {
                    Ok(())
                } else {
                    Err(ValidationError::TokenMismatch)
                }
            }
            _ => Err(ValidationError::UnauthorizedRole(
                "Message type not allowed for player connections",
            )),
        },
        Role::Beamer => match msg {
            ClientMessage::Join { .. } | ClientMessage::AckNeeded { .. } => Ok(()),
            _ => Err(ValidationError::UnauthorizedRole("Beamer is read-only")),
        },
        Role::Host => Ok(()),
    }
}

#[derive(Debug, Clone, Copy)]
enum ValidationError {
    TokenRequired(&'static str),
    TokenMismatch,
    UnauthorizedRole(&'static str),
}

impl ValidationError {
    fn to_server_message(self) -> ServerMessage {
        match self {
            ValidationError::TokenRequired(msg) => ServerMessage::Error {
                code: "TOKEN_REQUIRED".to_string(),
                msg: msg.to_string(),
            },
            ValidationError::TokenMismatch => ServerMessage::Error {
                code: "TOKEN_MISMATCH".to_string(),
                msg: "Ungültiger Token".to_string(),
            },
            ValidationError::UnauthorizedRole(msg) => ServerMessage::Error {
                code: "UNAUTHORIZED_ROLE".to_string(),
                msg: msg.to_string(),
            },
        }
    }
}

/// WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let role = match params.role.as_deref() {
        Some("host") => Role::Host,
        Some("beamer") => Role::Beamer,
        Some("player") => Role::Player,
        _ => Role::Audience,
    };

    // Block audience WebSocket connections during panic mode
    if role == Role::Audience && state.is_panic_mode().await {
        return (
            StatusCode::FORBIDDEN,
            "Audience connections temporarily disabled.",
        )
            .into_response();
    }

    // Block audience WebSocket connections during venue-only mode if IP not allowed
    if role == Role::Audience && state.is_venue_only_mode().await {
        let client_ip = extract_client_ip(&headers, &addr);
        if !state.is_ip_allowed_by_venue(client_ip) {
            let message = state.get_venue_rejection_message();
            tracing::info!(
                "Venue-only mode: rejected audience connection from IP {}",
                client_ip
            );
            return (StatusCode::FORBIDDEN, message).into_response();
        }
    }

    if token_required_for_role(&role) && params.token.is_none() {
        return (
            StatusCode::FORBIDDEN,
            "Missing token. Reconnect with ?token=... in the WebSocket URL.",
        )
            .into_response();
    }

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

    // Track connection count
    state.increment_connection(&role);

    let connection_token = params.token.as_deref();

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
        game: game.clone(),
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
                // Try to recover player state - validate token exists
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
                } else {
                    // Invalid player token - send error message
                    tracing::warn!("Invalid player token attempted: {}", token);
                    let error = ServerMessage::Error {
                        code: "INVALID_PLAYER_TOKEN".to_string(),
                        msg: "Lass dir bitte einen validen Token geben.".to_string(),
                    };
                    if let Ok(msg) = serde_json::to_string(&error) {
                        let _ = sender.send(Message::Text(msg.into())).await;
                    }
                }
            }
            Role::Audience => {
                // Get or create audience member with friendly display name
                let member = state.get_or_create_audience_member(token).await;

                // Try to recover audience vote state
                let vote = state.get_audience_vote_for_current_round(token).await;
                let audience_state = ServerMessage::AudienceState {
                    display_name: member.display_name.clone(),
                    has_voted: vote.is_some(),
                    current_vote: vote.map(|v| AudienceVoteInfo {
                        ai_pick: v.ai_pick_submission_id,
                        funny_pick: v.funny_pick_submission_id,
                    }),
                };
                if let Ok(msg) = serde_json::to_string(&audience_state) {
                    let _ = sender.send(Message::Text(msg.into())).await;
                }
                tracing::info!(
                    "Sent audience state recovery for token (name: {})",
                    member.display_name
                );

                // Send phase-specific state for audience
                match game.phase {
                    GamePhase::PromptSelection => {
                        // Send prompt candidates for voting
                        let queued = state.get_queued_prompts().await;
                        if !queued.is_empty() {
                            let candidates_msg =
                                ServerMessage::PromptCandidates { prompts: queued };
                            if let Ok(msg) = serde_json::to_string(&candidates_msg) {
                                let _ = sender.send(Message::Text(msg.into())).await;
                            }
                            tracing::info!("Sent prompt candidates for audience state recovery");
                        }

                        // Send audience prompt vote state if they've voted
                        if let Some(prompt_vote) = state.get_audience_prompt_vote(token).await {
                            let prompt_vote_state = ServerMessage::AudiencePromptVoteState {
                                has_voted: true,
                                voted_prompt_id: Some(prompt_vote),
                            };
                            if let Ok(msg) = serde_json::to_string(&prompt_vote_state) {
                                let _ = sender.send(Message::Text(msg.into())).await;
                            }
                            tracing::info!("Sent audience prompt vote state recovery");
                        }
                    }
                    GamePhase::Voting => {
                        // Send vote challenge for anti-automation
                        if let Some(nonce) = state.get_vote_challenge_nonce().await {
                            if let Some(round) = state.get_current_round().await {
                                let challenge_msg = ServerMessage::VoteChallenge {
                                    nonce,
                                    round_id: round.id.clone(),
                                };
                                if let Ok(msg) = serde_json::to_string(&challenge_msg) {
                                    let _ = sender.send(Message::Text(msg.into())).await;
                                }
                                tracing::info!(
                                    "Sent vote challenge for audience voting state recovery"
                                );
                            }
                        }

                        // Send submissions list for voting
                        if let Some(round) = state.get_current_round().await {
                            let submissions = state.get_submissions(&round.id).await;
                            let submissions_msg = ServerMessage::Submissions {
                                list: submissions.iter().map(SubmissionInfo::from).collect(),
                            };
                            if let Ok(msg) = serde_json::to_string(&submissions_msg) {
                                let _ = sender.send(Message::Text(msg.into())).await;
                            }
                            tracing::info!("Sent submissions for audience voting state recovery");
                        }
                    }
                    GamePhase::Writing => {
                        // Send active trivia question if any
                        if let Some(trivia) = state.get_active_trivia().await {
                            let choices: Vec<crate::protocol::TriviaChoiceOutput> = trivia
                                .choices
                                .iter()
                                .map(|c| crate::protocol::TriviaChoiceOutput {
                                    text: c.text.clone(),
                                    image_url: c.image_url.clone(),
                                })
                                .collect();
                            let trivia_msg = ServerMessage::TriviaQuestion {
                                question_id: trivia.id.clone(),
                                question: trivia.question.clone(),
                                image_url: trivia.image_url.clone(),
                                choices,
                            };
                            if let Ok(msg) = serde_json::to_string(&trivia_msg) {
                                let _ = sender.send(Message::Text(msg.into())).await;
                            }
                            tracing::info!("Sent trivia question for audience state recovery");

                            // Send their vote state if they've already voted
                            if let Some(vote) = state.get_trivia_vote(token).await {
                                let vote_state = ServerMessage::TriviaVoteState {
                                    question_id: trivia.id.clone(),
                                    has_voted: true,
                                    choice_index: Some(vote.choice_index),
                                };
                                if let Ok(msg) = serde_json::to_string(&vote_state) {
                                    let _ = sender.send(Message::Text(msg.into())).await;
                                }
                                tracing::info!(
                                    "Sent trivia vote state for audience state recovery"
                                );
                            }
                        }
                    }
                    GamePhase::Results | GamePhase::Podium => {
                        // Send scores for winner display (top 3 audience detection)
                        let (all_players, top_audience) = state.get_leaderboards().await;
                        let ai_submission_id = state
                            .get_current_round()
                            .await
                            .and_then(|r| r.ai_submission_id.clone());
                        let scores_msg = ServerMessage::Scores {
                            players: all_players,
                            audience_top: top_audience.into_iter().take(10).collect(),
                            ai_submission_id,
                        };
                        if let Ok(msg) = serde_json::to_string(&scores_msg) {
                            let _ = sender.send(Message::Text(msg.into())).await;
                        }
                        tracing::info!("Sent scores for audience state recovery (PODIUM/RESULTS)");
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // Send Beamer-specific state recovery
    if role == Role::Beamer {
        match game.phase {
            GamePhase::PromptSelection => {
                // Send prompt candidates for display
                let queued = state.get_queued_prompts().await;
                if !queued.is_empty() {
                    let candidates_msg = ServerMessage::PromptCandidates { prompts: queued };
                    if let Ok(msg) = serde_json::to_string(&candidates_msg) {
                        let _ = sender.send(Message::Text(msg.into())).await;
                    }

                    // Also send current vote counts
                    let counts = state.get_prompt_vote_counts().await;
                    let counts_msg = ServerMessage::BeamerPromptVoteCounts { counts };
                    if let Ok(msg) = serde_json::to_string(&counts_msg) {
                        let _ = sender.send(Message::Text(msg.into())).await;
                    }
                    tracing::info!("Sent prompt candidates and vote counts for beamer recovery");
                }
            }
            GamePhase::Voting => {
                // Send submissions list and current vote counts
                if let Some(round) = state.get_current_round().await {
                    let submissions = state.get_submissions(&round.id).await;
                    let submissions_msg = ServerMessage::Submissions {
                        list: submissions.iter().map(SubmissionInfo::from).collect(),
                    };
                    if let Ok(msg) = serde_json::to_string(&submissions_msg) {
                        let _ = sender.send(Message::Text(msg.into())).await;
                    }

                    // Send current vote counts
                    let (ai_counts, funny_counts) =
                        state.get_vote_counts_for_round(&round.id).await;
                    let vote_counts_msg = ServerMessage::BeamerVoteCounts {
                        ai: ai_counts,
                        funny: funny_counts,
                        seq: 0,
                    };
                    if let Ok(msg) = serde_json::to_string(&vote_counts_msg) {
                        let _ = sender.send(Message::Text(msg.into())).await;
                    }
                    tracing::info!("Sent submissions and vote counts for beamer voting recovery");
                }
            }
            GamePhase::Writing => {
                // Send current prompt for display
                if let Some(round) = state.get_current_round().await {
                    // Also send submission count without revealing texts
                    let count = state.get_submissions(&round.id).await.len() as u32;
                    let count_msg = ServerMessage::SubmissionCount { count };
                    if let Ok(msg) = serde_json::to_string(&count_msg) {
                        let _ = sender.send(Message::Text(msg.into())).await;
                    }

                    if let Some(prompt) = round.selected_prompt {
                        let prompt_msg = ServerMessage::PromptSelected { prompt };
                        if let Ok(msg) = serde_json::to_string(&prompt_msg) {
                            let _ = sender.send(Message::Text(msg.into())).await;
                        }
                        tracing::info!("Sent prompt for beamer writing phase recovery");
                    }
                }

                // Send active trivia question if any
                if let Some(trivia) = state.get_active_trivia().await {
                    let choices: Vec<crate::protocol::TriviaChoiceOutput> = trivia
                        .choices
                        .iter()
                        .map(|c| crate::protocol::TriviaChoiceOutput {
                            text: c.text.clone(),
                            image_url: c.image_url.clone(),
                        })
                        .collect();
                    let trivia_msg = ServerMessage::TriviaQuestion {
                        question_id: trivia.id.clone(),
                        question: trivia.question.clone(),
                        image_url: trivia.image_url.clone(),
                        choices,
                    };
                    if let Ok(msg) = serde_json::to_string(&trivia_msg) {
                        let _ = sender.send(Message::Text(msg.into())).await;
                    }
                    tracing::info!("Sent trivia question for beamer state recovery");
                }
            }
            GamePhase::Reveal => {
                // Send current reveal state (avoid sending full submissions list to prevent spoilers)
                if let Some(round) = state.get_current_round().await {
                    // Send submission count without revealing texts
                    let count = state.get_submissions(&round.id).await.len() as u32;
                    let count_msg = ServerMessage::SubmissionCount { count };
                    if let Ok(msg) = serde_json::to_string(&count_msg) {
                        let _ = sender.send(Message::Text(msg.into())).await;
                    }

                    // Send current reveal position
                    if let Some(submission) = state.get_current_reveal_submission(&round.id).await {
                        let reveal_msg = ServerMessage::RevealUpdate {
                            reveal_index: round.reveal_index,
                            submission: Some(SubmissionInfo::from(&submission)),
                        };
                        if let Ok(msg) = serde_json::to_string(&reveal_msg) {
                            let _ = sender.send(Message::Text(msg.into())).await;
                        }
                    }
                    tracing::info!("Sent reveal state for beamer recovery");
                }
            }
            GamePhase::Results | GamePhase::Podium => {
                // Send scores and submissions
                if let Some(round) = state.get_current_round().await {
                    let submissions = state.get_submissions(&round.id).await;
                    let submissions_msg = ServerMessage::Submissions {
                        list: submissions.iter().map(SubmissionInfo::from).collect(),
                    };
                    if let Ok(msg) = serde_json::to_string(&submissions_msg) {
                        let _ = sender.send(Message::Text(msg.into())).await;
                    }

                    // Send vote counts for results display
                    let (ai_counts, funny_counts) =
                        state.get_vote_counts_for_round(&round.id).await;
                    let vote_counts_msg = ServerMessage::BeamerVoteCounts {
                        ai: ai_counts,
                        funny: funny_counts,
                        seq: 0,
                    };
                    if let Ok(msg) = serde_json::to_string(&vote_counts_msg) {
                        let _ = sender.send(Message::Text(msg.into())).await;
                    }

                    // Send manual winners if set (for panic mode)
                    if round.manual_ai_winner.is_some() || round.manual_funny_winner.is_some() {
                        let manual_winners_msg = ServerMessage::ManualWinners {
                            ai_winner_id: round.manual_ai_winner.clone(),
                            funny_winner_id: round.manual_funny_winner.clone(),
                        };
                        if let Ok(msg) = serde_json::to_string(&manual_winners_msg) {
                            let _ = sender.send(Message::Text(msg.into())).await;
                        }
                    }
                }

                // Send scores
                let (all_players, top_audience) = state.get_leaderboards().await;
                let ai_submission_id = state
                    .get_current_round()
                    .await
                    .and_then(|r| r.ai_submission_id.clone());
                let scores_msg = ServerMessage::Scores {
                    players: all_players,
                    audience_top: top_audience.into_iter().take(10).collect(),
                    ai_submission_id,
                };
                if let Ok(msg) = serde_json::to_string(&scores_msg) {
                    let _ = sender.send(Message::Text(msg.into())).await;
                }
                tracing::info!("Sent results/podium state for beamer recovery");
            }
            _ => {}
        }
    }

    // Send host-specific state recovery
    if role == Role::Host {
        // Send current prompts pool (independent of rounds)
        let prompts = state.get_prompts_for_host().await;
        let stats = state.compute_prompt_pool_stats().await;
        let host_prompts = ServerMessage::HostPrompts { prompts, stats };
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

        // Send trivia questions and active trivia state
        let trivia_questions = state.get_trivia_questions().await;
        let active_trivia_id = state.get_active_trivia_id().await;
        let active_trivia_votes = state.get_active_trivia_vote_count().await;
        let trivia_msg = ServerMessage::HostTriviaQuestions {
            questions: trivia_questions,
            active_trivia_id,
            active_trivia_votes,
        };
        if let Ok(msg) = serde_json::to_string(&trivia_msg) {
            let _ = sender.send(Message::Text(msg.into())).await;
        }

        // Send venue-only mode status
        let venue_msg = ServerMessage::VenueOnlyModeUpdate {
            enabled: state.is_venue_only_mode().await,
        };
        if let Ok(msg) = serde_json::to_string(&venue_msg) {
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

    // Subscribe to audience disconnect signal if Audience
    let mut audience_disconnect_rx = if role == Role::Audience {
        Some(state.audience_disconnect.subscribe())
    } else {
        None
    };

    // Handle incoming messages and broadcasts
    // Use connection token for rate limiting, or fallback for anonymous connections
    let rate_limit_key = connection_token.unwrap_or("anonymous").to_string();
    loop {
        tokio::select! {
            // Handle audience disconnect signal (panic mode)
            disconnect_signal = async {
                match &mut audience_disconnect_rx {
                    Some(rx) => rx.recv().await.ok(),
                    None => std::future::pending::<Option<()>>().await
                }
            } => {
                if disconnect_signal.is_some() {
                    tracing::info!("Audience connection disconnected due to panic mode");
                    let _ = sender.send(Message::Close(Some(
                        axum::extract::ws::CloseFrame {
                            code: 1000,
                            reason: "Panic mode enabled".into(),
                        }
                    ))).await;
                    break;
                }
            }

            // Handle general broadcasts (all clients)
            broadcast_msg = broadcast_rx.recv() => {
                if let Ok(msg) = broadcast_msg {
                    if send_json(&mut sender, &msg).await.is_err() {
                        break;
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
                    if send_json(&mut sender, &msg).await.is_err() {
                        break;
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
                    if send_json(&mut sender, &msg).await.is_err() {
                        break;
                    }
                }
            }

            // Handle client messages
            ws_msg = receiver.next() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        if text.len() > MAX_WS_MESSAGE_BYTES {
                            let error = ServerMessage::Error {
                                code: "MESSAGE_TOO_LARGE".to_string(),
                                msg: "Nachricht zu groß".to_string(),
                            };
                            if let Ok(json) = serde_json::to_string(&error) {
                                let _ = sender.send(Message::Text(json.into())).await;
                            }
                            break;
                        }

                        // Rate limit non-trusted roles (skip for host/beamer)
                        if role != Role::Host
                            && role != Role::Beamer
                            && !state.check_rate_limit(&rate_limit_key).await
                        {
                            let error = ServerMessage::Error {
                                code: "RATE_LIMITED".to_string(),
                                msg: "Zu viele Nachrichten".to_string(),
                            };
                            if let Ok(json) = serde_json::to_string(&error) {
                                let _ = sender.send(Message::Text(json.into())).await;
                            }
                            break;
                        }

                        tracing::debug!("Received message: {}", text);

                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(client_msg) => {
                                if let Err(err_msg) = validate_message_for_role(
                                    &role,
                                    connection_token,
                                    &client_msg,
                                ) {
                                    let err_msg = err_msg.to_server_message();
                                    if let Ok(json) = serde_json::to_string(&err_msg) {
                                        let _ = sender.send(Message::Text(json.into())).await;
                                    }
                                    continue;
                                }

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
                                    msg: format!("Ungültiges Format: {}", e),
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

    // Decrement connection count
    state.decrement_connection(&role);

    tracing::info!("WebSocket connection closed for role: {:?}", role);
}
