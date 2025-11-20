use crate::protocol::{ClientMessage, ServerMessage};
use crate::state::AppState;
use crate::types::Role;
use std::sync::Arc;

/// Handle client messages and return optional response
pub async fn handle_message(
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
        } => handle_register_player(state, player_token, display_name).await,

        ClientMessage::SubmitAnswer { player_token, text } => {
            handle_submit_answer(state, player_token, text).await
        }

        ClientMessage::Vote {
            voter_token,
            ai,
            funny,
            msg_id,
        } => handle_vote(state, voter_token, ai, funny, msg_id).await,

        ClientMessage::SubmitPrompt { text } => handle_submit_prompt(state, text).await,

        ClientMessage::AckNeeded {
            last_seen_server_seq,
        } => {
            tracing::info!("Ack needed: seq={}", last_seen_server_seq);
            None
        }

        // Host-only commands
        ClientMessage::HostCreatePlayers { count } => {
            if *role != Role::Host {
                return unauthorized("Only host can create players");
            }
            handle_host_create_players(state, count).await
        }

        ClientMessage::HostTransitionPhase { phase } => {
            if *role != Role::Host {
                return unauthorized("Only host can transition phases");
            }
            handle_host_transition_phase(state, phase).await
        }

        ClientMessage::HostStartRound => {
            if *role != Role::Host {
                return unauthorized("Only host can start rounds");
            }
            handle_host_start_round(state).await
        }

        ClientMessage::HostSelectPrompt { prompt_id } => {
            if *role != Role::Host {
                return unauthorized("Only host can select prompts");
            }
            handle_host_select_prompt(state, prompt_id).await
        }

        ClientMessage::HostEditSubmission {
            submission_id,
            new_text,
        } => {
            if *role != Role::Host {
                return unauthorized("Only host can edit submissions");
            }
            handle_host_edit_submission(state, submission_id, new_text).await
        }

        ClientMessage::HostSetRevealOrder { order } => {
            if *role != Role::Host {
                return unauthorized("Only host can set reveal order");
            }
            handle_host_set_reveal_order(state, order).await
        }

        ClientMessage::HostSetAiSubmission { submission_id } => {
            if *role != Role::Host {
                return unauthorized("Only host can set AI submission");
            }
            handle_host_set_ai_submission(state, submission_id).await
        }

        ClientMessage::HostRevealNext => {
            if *role != Role::Host {
                return unauthorized("Only host can control reveal");
            }
            handle_host_reveal_next(state).await
        }

        ClientMessage::HostRevealPrev => {
            if *role != Role::Host {
                return unauthorized("Only host can control reveal");
            }
            handle_host_reveal_prev(state).await
        }

        ClientMessage::HostResetGame => {
            if *role != Role::Host {
                return unauthorized("Only host can reset game");
            }
            handle_host_reset_game(state).await
        }
    }
}

fn unauthorized(msg: &str) -> Option<ServerMessage> {
    Some(ServerMessage::Error {
        code: "UNAUTHORIZED".to_string(),
        msg: msg.to_string(),
    })
}

async fn handle_register_player(
    state: &Arc<AppState>,
    player_token: String,
    display_name: String,
) -> Option<ServerMessage> {
    tracing::info!("Player registration: {}", display_name);
    match state
        .register_player(&player_token, display_name.clone())
        .await
    {
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

async fn handle_submit_answer(
    state: &Arc<AppState>,
    player_token: Option<String>,
    text: String,
) -> Option<ServerMessage> {
    tracing::info!("Answer submitted: {}", text);
    let round = state.get_current_round().await?;

    let player_id = if let Some(token) = player_token {
        match state.get_player_by_token(&token).await {
            Some(player) => Some(player.id),
            None => {
                return Some(ServerMessage::Error {
                    code: "INVALID_PLAYER_TOKEN".to_string(),
                    msg: "Invalid player token".to_string(),
                });
            }
        }
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
}

async fn handle_vote(
    state: &Arc<AppState>,
    voter_token: String,
    ai: String,
    funny: String,
    msg_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Vote: AI={}, Funny={}, MsgID={}", ai, funny, msg_id);
    if let Some(round) = state.get_current_round().await {
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

async fn handle_submit_prompt(state: &Arc<AppState>, text: String) -> Option<ServerMessage> {
    tracing::info!("Prompt submitted: {}", text);
    let round = state.get_current_round().await?;

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
}

async fn handle_host_create_players(state: &Arc<AppState>, count: u32) -> Option<ServerMessage> {
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

async fn handle_host_transition_phase(
    state: &Arc<AppState>,
    phase: crate::types::GamePhase,
) -> Option<ServerMessage> {
    tracing::info!("Host transitioning to phase: {:?}", phase);
    match state.transition_phase(phase).await {
        Ok(_) => state
            .get_game()
            .await
            .map(|game| ServerMessage::GameState { game }),
        Err(e) => Some(ServerMessage::Error {
            code: "TRANSITION_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_start_round(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host starting new round");
    match state.start_round().await {
        Ok(round) => Some(ServerMessage::RoundStarted { round }),
        Err(e) => Some(ServerMessage::Error {
            code: "ROUND_START_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_select_prompt(
    state: &Arc<AppState>,
    prompt_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host selecting prompt: {}", prompt_id);
    let round = state.get_current_round().await?;

    match state.select_prompt(&round.id, &prompt_id).await {
        Ok(_) => {
            let updated_round = state.get_current_round().await?;
            updated_round
                .selected_prompt
                .map(|prompt| ServerMessage::PromptSelected { prompt })
        }
        Err(e) => Some(ServerMessage::Error {
            code: "PROMPT_SELECT_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_edit_submission(
    state: &Arc<AppState>,
    submission_id: String,
    new_text: String,
) -> Option<ServerMessage> {
    tracing::info!("Host editing submission: {}", submission_id);
    match state.edit_submission(&submission_id, new_text).await {
        Ok(_) => None,
        Err(e) => Some(ServerMessage::Error {
            code: "EDIT_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_set_reveal_order(
    state: &Arc<AppState>,
    order: Vec<String>,
) -> Option<ServerMessage> {
    tracing::info!("Host setting reveal order: {} items", order.len());
    let round = state.get_current_round().await?;

    match state.set_reveal_order(&round.id, order).await {
        Ok(_) => None,
        Err(e) => Some(ServerMessage::Error {
            code: "REVEAL_ORDER_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_set_ai_submission(
    state: &Arc<AppState>,
    submission_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host setting AI submission: {}", submission_id);
    let round = state.get_current_round().await?;

    match state.set_ai_submission(&round.id, submission_id).await {
        Ok(_) => None,
        Err(e) => Some(ServerMessage::Error {
            code: "SET_AI_SUBMISSION_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_reveal_next(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host advancing reveal");
    let round = state.get_current_round().await?;

    match state.reveal_next(&round.id).await {
        Ok(reveal_index) => {
            // Get the submission at the current reveal index
            let submission = state.get_current_reveal_submission(&round.id).await;
            let submission_info = submission.map(|s| crate::protocol::SubmissionInfo::from(&s));

            // Broadcast to all clients
            state.broadcast_to_all(ServerMessage::RevealUpdate {
                reveal_index,
                submission: submission_info.clone(),
            });

            Some(ServerMessage::RevealUpdate {
                reveal_index,
                submission: submission_info,
            })
        }
        Err(e) => Some(ServerMessage::Error {
            code: "REVEAL_NEXT_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_reveal_prev(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host going back in reveal");
    let round = state.get_current_round().await?;

    match state.reveal_prev(&round.id).await {
        Ok(reveal_index) => {
            // Get the submission at the current reveal index
            let submission = state.get_current_reveal_submission(&round.id).await;
            let submission_info = submission.map(|s| crate::protocol::SubmissionInfo::from(&s));

            // Broadcast to all clients
            state.broadcast_to_all(ServerMessage::RevealUpdate {
                reveal_index,
                submission: submission_info.clone(),
            });

            Some(ServerMessage::RevealUpdate {
                reveal_index,
                submission: submission_info,
            })
        }
        Err(e) => Some(ServerMessage::Error {
            code: "REVEAL_PREV_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_reset_game(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host resetting game");
    state.reset_game().await;

    // Return game state after reset
    let game = state.get_game().await?;
    Some(ServerMessage::GameState { game })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GamePhase;

    #[tokio::test]
    async fn test_unauthorized_host_command() {
        let state = Arc::new(AppState::new());
        let role = Role::Audience;

        let result =
            handle_message(ClientMessage::HostCreatePlayers { count: 3 }, &role, &state).await;

        assert!(result.is_some());
        if let Some(ServerMessage::Error { code, .. }) = result {
            assert_eq!(code, "UNAUTHORIZED");
        }
    }

    #[tokio::test]
    async fn test_host_create_players() {
        let state = Arc::new(AppState::new());
        let role = Role::Host;

        let result =
            handle_message(ClientMessage::HostCreatePlayers { count: 2 }, &role, &state).await;

        assert!(result.is_some());
        if let Some(ServerMessage::PlayersCreated { players }) = result {
            assert_eq!(players.len(), 2);
        } else {
            panic!("Expected PlayersCreated message");
        }
    }

    #[tokio::test]
    async fn test_phase_transition() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let role = Role::Host;

        // Test valid transition: Lobby -> PromptSelection
        let result = handle_message(
            ClientMessage::HostTransitionPhase {
                phase: GamePhase::PromptSelection,
            },
            &role,
            &state,
        )
        .await;

        assert!(result.is_some());
        if let Some(ServerMessage::GameState { game }) = result {
            assert_eq!(game.phase, GamePhase::PromptSelection);
        } else {
            panic!("Expected GameState message");
        }
    }

    #[tokio::test]
    async fn test_reset_game() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let role = Role::Host;

        // Create some players
        handle_message(ClientMessage::HostCreatePlayers { count: 3 }, &role, &state).await;

        // Transition to a different phase
        state
            .transition_phase(GamePhase::PromptSelection)
            .await
            .ok();

        // Verify state is not empty
        let players = state.players.read().await;
        assert_eq!(players.len(), 3);
        drop(players);

        let game_before = state.get_game().await.unwrap();
        assert_eq!(game_before.phase, GamePhase::PromptSelection);

        // Reset the game
        let result = handle_message(ClientMessage::HostResetGame, &role, &state).await;

        // Verify response
        assert!(result.is_some());
        if let Some(ServerMessage::GameState { game }) = result {
            assert_eq!(game.phase, GamePhase::Lobby);
            assert_eq!(game.round_no, 0);
            assert_eq!(game.current_round_id, None);
        } else {
            panic!("Expected GameState message");
        }

        // Verify all state is cleared
        let players = state.players.read().await;
        assert_eq!(players.len(), 0);
        drop(players);

        let rounds = state.rounds.read().await;
        assert_eq!(rounds.len(), 0);
        drop(rounds);

        let submissions = state.submissions.read().await;
        assert_eq!(submissions.len(), 0);
        drop(submissions);

        let votes = state.votes.read().await;
        assert_eq!(votes.len(), 0);
        drop(votes);

        let scores = state.scores.read().await;
        assert_eq!(scores.len(), 0);
    }

    #[tokio::test]
    async fn test_reset_game_requires_host() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let role = Role::Audience;

        let result = handle_message(ClientMessage::HostResetGame, &role, &state).await;

        assert!(result.is_some());
        if let Some(ServerMessage::Error { code, .. }) = result {
            assert_eq!(code, "UNAUTHORIZED");
        } else {
            panic!("Expected Error message");
        }
    }
}
